package org.xinutec.life

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.util.Base64
import android.util.Log
import android.view.Gravity
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.FrameLayout
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * A full-screen [WebView] onto life — the personal home-OS app, an Angular SPA
 * served at [LIFE_URL]. No address bar, no tabs, a home-screen icon: the app
 * presented as a native one, avoiding browser chrome. It's behind a login
 * (Nextcloud identity); the WebView keeps the session cookie, so it's a one-time
 * sign-in.
 *
 * Deliberately tiny — a plain Activity holding one WebView, no Compose/AppCompat.
 * `configChanges` keeps the WebView (and its route + scroll) across rotation.
 *
 * The WebView is inset from the system bars by padding a wrapper (see onCreate),
 * and the strips behind the bars are painted with the page's own surface colour.
 */
class MainActivity : Activity() {
    private lateinit var web: WebView
    private lateinit var root: FrameLayout

    // A pending web camera request, held while the OS permission dialog is up.
    private var pendingCameraRequest: PermissionRequest? = null

    // A pending <input type=file> result callback, held while the picker is open.
    private var fileChooserCallback: ValueCallback<Array<Uri>>? = null

    // The offscreen WebView doing a shop lookup, if one is in flight. One at a
    // time; a new request tears down the previous.
    private var shopWeb: WebView? = null

    // The visible shop-login overlay + its WebView, and the pending connect
    // request to answer when it closes.
    private var connectOverlay: FrameLayout? = null
    private var connectWeb: WebView? = null
    private var connectRequestId: String? = null

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Make both WebViews inspectable over adb (chrome://inspect / CDP). The app
        // is a personal, sideloaded debug build; this is how in-app web + the hidden
        // Waitrose fetch get diagnosed (the view isn't otherwise remote-debuggable).
        WebView.setWebContentsDebuggingEnabled(true)
        val prefs = getSharedPreferences("viewer", Context.MODE_PRIVATE)
        web =
            WebView(this).apply {
                layoutParams =
                    ViewGroup.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT,
                    )
                settings.javaScriptEnabled = true // Angular needs JS
                settings.domStorageEnabled = true // localStorage / sessionStorage
                settings.useWideViewPort = true
                settings.loadWithOverviewMode = true
                // Expose the system clipboard's image to the web app (its "Paste
                // copied image" action — e.g. an image copied in Chrome). The
                // bridge object is attached to the WebView as a whole, so every
                // call re-checks that the *current page* is the life app (see
                // readClipboardImageDataUrl) — a foreign page can't read the
                // clipboard even if it somehow ends up in this view.
                addJavascriptInterface(ClipboardImageBridge(), "AndroidClipboard")
                // Shop enrichment: the web app drives a hidden WebView on a shop
                // site (a real browser passes the bot wall a server-side client
                // can't) to fetch product data, supplying the shop-specific URLs +
                // extractor JS. See ShopBridge — nothing shop-specific lives here.
                addJavascriptInterface(ShopBridge(), "ShopBridge")
                // Keep life (and its Nextcloud login hop) inside this WebView;
                // hand every other origin to the real browser. A chromeless view
                // has no URL bar, so an external link opening in-place would look
                // like the app — confine navigation instead. Also remember the
                // current in-app page so a cold reopen returns to it (SPA route
                // changes fire doUpdateVisitedHistory too).
                webViewClient =
                    object : WebViewClient() {
                        override fun shouldOverrideUrlLoading(
                            view: WebView,
                            request: WebResourceRequest,
                        ): Boolean {
                            val url = request.url
                            if (url.scheme == "https" && url.host in ALLOWED_HOSTS) {
                                return false // in-app
                            }
                            try {
                                startActivity(Intent(Intent.ACTION_VIEW, url))
                            } catch (_: ActivityNotFoundException) {
                                // No handler for this URL — drop the navigation.
                            }
                            return true
                        }

                        override fun doUpdateVisitedHistory(
                            view: WebView,
                            url: String,
                            isReload: Boolean,
                        ) {
                            super.doUpdateVisitedHistory(view, url, isReload)
                            if (url.startsWith(LIFE_URL)) {
                                prefs.edit().putString(KEY_LAST_URL, url).apply()
                            }
                        }

                        // Paint the strips behind the system bars with the web UI's
                        // own surface colour instead of a hardcoded black; it follows
                        // the page's light/dark theme, so read its body background.
                        override fun onPageFinished(view: WebView, url: String) {
                            super.onPageFinished(view, url)
                            view.evaluateJavascript(
                                "getComputedStyle(document.body).backgroundColor",
                            ) { result -> parseCssColor(result)?.let(root::setBackgroundColor) }
                        }
                    }
                // The barcode scanner calls getUserMedia; a WebView denies camera
                // access unless we explicitly grant it. Grant video capture, asking
                // the OS for the runtime CAMERA permission first if we lack it.
                webChromeClient =
                    object : WebChromeClient() {
                        // Mirror the web app's console to logcat (tag "life-web") so the
                        // in-WebView flow — e.g. the scanner's "[scan]" traces — is
                        // visible via `adb logcat -s life-web`.
                        override fun onConsoleMessage(msg: ConsoleMessage): Boolean {
                            Log.d(
                                "life-web",
                                "${msg.message()} (${msg.sourceId()}:${msg.lineNumber()})",
                            )
                            return true
                        }

                        override fun onPermissionRequest(request: PermissionRequest) {
                            if (PermissionRequest.RESOURCE_VIDEO_CAPTURE !in request.resources) {
                                request.deny()
                                return
                            }
                            if (hasCameraPermission()) {
                                request.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE))
                            } else {
                                pendingCameraRequest = request
                                requestPermissions(arrayOf(Manifest.permission.CAMERA), CAMERA_REQ)
                            }
                        }

                        // A WebView ignores <input type=file> unless we launch the
                        // picker ourselves and hand the chosen URIs back — without
                        // this, tapping the app's image picker does nothing (it works
                        // in Chrome, which supplies its own file dialog). The intent
                        // from createIntent() honours the input's `accept` (image/*)
                        // and `multiple`, so it opens straight to the photo picker.
                        override fun onShowFileChooser(
                            webView: WebView,
                            filePathCallback: ValueCallback<Array<Uri>>,
                            fileChooserParams: FileChooserParams,
                        ): Boolean {
                            // Abandon any earlier pick that never resolved.
                            fileChooserCallback?.onReceiveValue(null)
                            fileChooserCallback = filePathCallback
                            return try {
                                startActivityForResult(fileChooserParams.createIntent(), FILE_REQ)
                                true
                            } catch (_: ActivityNotFoundException) {
                                fileChooserCallback = null
                                false // let the WebView know no chooser was shown
                            }
                        }
                    }
                // Black until the page loads and reports its surface colour; avoids a
                // white flash on launch.
                setBackgroundColor(Color.BLACK)
            }
        // Inset the WebView from the system bars by padding a wrapper ViewGroup
        // (WebView.setPadding() doesn't offset content under wide-viewport mode).
        // Once the WebView no longer underlaps the bars its env(safe-area-inset-*)
        // collapse to 0, so the page's own safe-area CSS adds nothing on top.
        root =
            FrameLayout(this).apply {
                addView(web)
                setBackgroundColor(Color.BLACK)
            }
        ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
            // ime() included: with enforced edge-to-edge (targetSdk 35+) the window
            // no longer auto-resizes for the keyboard — without this the IME just
            // draws over the page and bottom sheets stay buried under it.
            val bars =
                insets.getInsets(
                    WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime(),
                )
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        setContentView(root)
        // Reopen where we left off; the hardcoded URL is only the first-run default.
        web.loadUrl(prefs.getString(KEY_LAST_URL, null) ?: LIFE_URL)
    }

    // `configChanges` keeps the Activity across rotation, so this only fires on a
    // real finish — release the WebView instead of leaking it.
    override fun onDestroy() {
        shopWeb?.let {
            root.removeView(it)
            it.destroy()
        }
        connectOverlay?.let { root.removeView(it) }
        connectWeb?.destroy()
        root.removeView(web)
        web.destroy()
        super.onDestroy()
    }

    // Back walks the SPA's history; it only leaves the app once there's nothing
    // left to go back to.
    @Deprecated("Deprecated in Java")
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        // The Waitrose connect overlay swallows back first: walk its history, then
        // close it, before the main app's back behaviour.
        val cw = connectWeb
        if (connectOverlay != null) {
            if (cw != null && cw.canGoBack()) cw.goBack() else closeShopConnect()
            return
        }
        if (web.canGoBack()) web.goBack() else super.onBackPressed()
    }

    // Deliver the picked image URIs back to the waiting <input type=file>. The
    // callback MUST be answered even on cancel (null), or the input stays blocked
    // and won't reopen the picker on the next tap.
    @Deprecated("Deprecated in Java")
    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != FILE_REQ) return
        val callback = fileChooserCallback ?: return
        fileChooserCallback = null
        callback.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(resultCode, data))
    }

    private fun hasCameraPermission() =
        checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

    /**
     * Bridge for the web app's "Paste copied image": returns the image on the
     * system clipboard as a `data:` URL, or null if there's no image. A WebView
     * can't read a clipboard image itself, so the page asks us. Reading happens
     * on the UI thread (ClipboardManager requires it) even though @JavascriptInterface
     * calls arrive on a binder thread.
     */
    inner class ClipboardImageBridge {
        @JavascriptInterface
        fun readImage(): String? {
            val task = FutureTask { readClipboardImageDataUrl() }
            runOnUiThread(task)
            return try {
                task.get(2, TimeUnit.SECONDS)
            } catch (_: Exception) {
                null
            }
        }
    }

    private fun readClipboardImageDataUrl(): String? {
        // Origin gate (runs on the UI thread, so web.url is safe to read): only
        // the life app itself may read the clipboard — not the NC login page,
        // and not any page that might slip past navigation confinement.
        if (web.url?.startsWith(LIFE_URL) != true) return null
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val clip = clipboard.primaryClip ?: return null
        for (i in 0 until clip.itemCount) {
            val uri = clip.getItemAt(i).uri ?: continue
            val mime = contentResolver.getType(uri)?.takeIf { it.startsWith("image/") } ?: continue
            val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: continue
            if (bytes.size > MAX_PASTE_BYTES) return null // let the backend cap stand
            return "data:$mime;base64," + Base64.encodeToString(bytes, Base64.NO_WRAP)
        }
        return null
    }

    /**
     * Generic shop-enrichment bridge. The web app supplies per-shop URLs and
     * extractor JS (Waitrose, Asda, …); this layer knows nothing shop-specific. It
     * loads a shop URL in a hidden WebView — a real browser, so it passes the
     * shop's bot-wall a server-side client can't — captures any Bearer token the
     * page attaches (into window.__authToken), runs the supplied JS, and reports
     * its `AndroidShop.result(json)` back via window.__shopResolve(requestId, …).
     * `available()` lets the web app feature-detect (absent in a plain browser).
     */
    inner class ShopBridge {
        @JavascriptInterface fun available(): Boolean = true

        // Load `url`, run `extractorJs` once loaded, resolve window.__shopResolve.
        @JavascriptInterface
        fun run(url: String, extractorJs: String, requestId: String) {
            runOnUiThread { shopRun(url, extractorJs, requestId) }
        }

        // Show the visible shop login so a session is established (cookies persist
        // in the shared jar; a hidden fetch needs a logged-in SPA). Notifies
        // window.__shopConnected(requestId) when the overlay closes.
        @JavascriptInterface
        fun connect(loginUrl: String, requestId: String) {
            runOnUiThread { showShopConnect(loginUrl, requestId) }
        }
    }

    /**
     * Load a shop page in a throwaway offscreen WebView and run the web-app-
     * supplied [extractorJs] once it finishes. Why a WebView, not an HTTP client:
     * shop sites sit behind bot managers (Akamai etc.) that reject non-browser
     * TLS/HTTP2 fingerprints — only a real browser engine passes. A generic capture
     * patch grabs any Bearer token the page attaches (window.__authToken) for the
     * extractor to use. Results return through the per-view AndroidShop bridge
     * because WebView.evaluateJavascript does NOT await promises. One at a time.
     */
    @SuppressLint("SetJavaScriptEnabled")
    private fun shopRun(url: String, extractorJs: String, requestId: String) {
        if (web.url?.startsWith(LIFE_URL) != true) {
            resolveShop(requestId, """{"ok":false,"error":"forbidden"}""")
            return
        }
        if (!isShopUrl(url)) {
            resolveShop(requestId, """{"ok":false,"error":"host not allowed"}""")
            return
        }
        shopWeb?.let {
            root.removeView(it)
            it.destroy()
        }
        val hidden =
            WebView(this).apply {
                settings.javaScriptEnabled = true
                settings.domStorageEnabled = true
                // Offscreen but attached (1×1) so JS timers/network run reliably.
                layoutParams = FrameLayout.LayoutParams(1, 1)
            }
        // A shop SPA's session/consent flow leans on third-party cookies, which a
        // WebView blocks by default.
        CookieManager.getInstance().setAcceptThirdPartyCookies(hidden, true)
        shopWeb = hidden
        root.addView(hidden)

        val settled = AtomicBoolean(false)
        val finish = { payload: String ->
            if (settled.compareAndSet(false, true)) {
                resolveShop(requestId, payload)
                hidden.post {
                    root.removeView(hidden)
                    hidden.destroy()
                    if (shopWeb === hidden) shopWeb = null
                }
            }
        }
        hidden.addJavascriptInterface(
            object {
                @JavascriptInterface fun result(json: String) = runOnUiThread { finish(json) }
            },
            "AndroidShop",
        )
        hidden.webChromeClient =
            object : WebChromeClient() {
                override fun onConsoleMessage(msg: ConsoleMessage): Boolean {
                    Log.d("life-shop", "${msg.message()} (${msg.sourceId()}:${msg.lineNumber()})")
                    return true
                }
            }
        hidden.webViewClient =
            object : WebViewClient() {
                // Patch fetch/XHR early, before the SPA fires its authed calls, so
                // we catch any Bearer token it attaches.
                override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                    view.evaluateJavascript(SHOP_CAPTURE_JS, null)
                }

                override fun onPageFinished(view: WebView, url: String) {
                    view.evaluateJavascript(extractorJs, null)
                }

                override fun onReceivedError(
                    view: WebView,
                    request: WebResourceRequest,
                    error: android.webkit.WebResourceError,
                ) {
                    if (request.isForMainFrame) finish("""{"ok":false,"error":"load failed"}""")
                }
            }
        // Safety net: never leave the web app's promise hanging.
        hidden.postDelayed({ finish("""{"ok":false,"error":"timeout"}""") }, SHOP_TIMEOUT_MS)
        hidden.loadUrl(url)
    }

    /** Whether `raw` is an https URL on an allowlisted shop host. */
    private fun isShopUrl(raw: String): Boolean {
        val u =
            try {
                Uri.parse(raw)
            } catch (_: Exception) {
                return false
            }
        if (u.scheme != "https") return false
        val host = u.host ?: return false
        return SHOP_HOSTS.any { host == it || host.endsWith(".$it") }
    }

    /** Resolve the web app's pending promise with a JSON result object. */
    private fun resolveShop(requestId: String, resultJson: String) {
        val js =
            "window.__shopResolve && window.__shopResolve(${JSONObject.quote(
                requestId,
            )}, $resultJson)"
        web.post { web.evaluateJavascript(js, null) }
    }

    /**
     * Show a full-screen shop WebView (at [loginUrl]) so the user signs in once.
     * The session cookies land in the shared CookieManager, so the hidden fetch
     * view (and any future basket/order calls) inherit a logged-in session. A
     * "Done" button and the back key close it; the web app is notified via
     * window.__shopConnected(requestId).
     */
    @SuppressLint("SetJavaScriptEnabled")
    private fun showShopConnect(loginUrl: String, requestId: String) {
        if (connectOverlay != null) return // already open
        if (!isShopUrl(loginUrl)) {
            notifyShopConnected(requestId)
            return
        }
        connectRequestId = requestId
        val cw =
            WebView(this).apply {
                layoutParams =
                    FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT,
                    )
                settings.javaScriptEnabled = true
                settings.domStorageEnabled = true
                settings.useWideViewPort = true
                settings.loadWithOverviewMode = true
                webChromeClient =
                    object : WebChromeClient() {
                        override fun onConsoleMessage(msg: ConsoleMessage): Boolean {
                            Log.d("life-shop", "connect: ${msg.message()}")
                            return true
                        }
                    }
                webViewClient =
                    object : WebViewClient() {
                        // Keep shop hosts in this view; hand anything else to the browser.
                        override fun shouldOverrideUrlLoading(
                            view: WebView,
                            request: WebResourceRequest,
                        ): Boolean {
                            if (isShopUrl(request.url.toString())) return false
                            try {
                                startActivity(Intent(Intent.ACTION_VIEW, request.url))
                            } catch (_: ActivityNotFoundException) {
                            }
                            return true
                        }
                    }
            }
        CookieManager.getInstance().setAcceptThirdPartyCookies(cw, true)
        connectWeb = cw

        // This WebView shows a *third-party* retailer's login page (loginUrl), not
        // life's own web app, so life can't inject a Done control into it — a native
        // escape button is the correct design here, not web chrome.
        val done =
            Button(this).apply {
                // dev-lint: android-native-chrome allow — external login overlay
                text = "Done"
                setOnClickListener { closeShopConnect() }
                layoutParams =
                    FrameLayout
                        .LayoutParams(
                            ViewGroup.LayoutParams.WRAP_CONTENT,
                            ViewGroup.LayoutParams.WRAP_CONTENT,
                        ).apply { gravity = Gravity.TOP or Gravity.END }
            }
        val overlay =
            FrameLayout(this).apply {
                layoutParams =
                    FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT,
                    )
                setBackgroundColor(Color.WHITE)
                addView(cw)
                addView(done)
            }
        connectOverlay = overlay
        root.addView(overlay)
        cw.loadUrl(loginUrl)
    }

    private fun closeShopConnect() {
        val overlay = connectOverlay ?: return
        connectOverlay = null
        root.removeView(overlay)
        connectWeb?.destroy()
        connectWeb = null
        val id = connectRequestId
        connectRequestId = null
        // Let the web app re-check / retry now a session may exist.
        notifyShopConnected(id)
    }

    /** Notify the web app that a connect overlay closed (requestId may be null). */
    private fun notifyShopConnected(requestId: String?) {
        val arg = requestId?.let { JSONObject.quote(it) } ?: "null"
        web.evaluateJavascript("window.__shopConnected && window.__shopConnected($arg)", null)
    }

    // Resolve the held web camera request once the user answers the OS dialog.
    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != CAMERA_REQ) return
        val request = pendingCameraRequest ?: return
        pendingCameraRequest = null
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            request.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE))
        } else {
            request.deny()
        }
    }

    // evaluateJavascript hands back the JSON-encoded result, e.g. the string
    // "rgb(18, 18, 18)" (with quotes) or "rgba(18, 18, 18, 1)". Pull out the RGB
    // triple; alpha is ignored (the surface is opaque). null if it can't be read.
    private fun parseCssColor(raw: String?): Int? {
        val m = raw?.let { Regex("""rgba?\((\d+),\s*(\d+),\s*(\d+)""").find(it) } ?: return null
        val (r, g, b) = m.destructured
        return Color.rgb(r.toInt(), g.toInt(), b.toInt())
    }

    companion object {
        private const val CAMERA_REQ = 1
        private const val FILE_REQ = 2

        // Skip pasting anything larger than the backend's 5 MiB image cap.
        private const val MAX_PASTE_BYTES = 5 * 1024 * 1024

        // The life app (HTTPS, behind a Nextcloud-identity login).
        private const val LIFE_URL = "https://life.xinutec.org/"

        // Hosts allowed to load inside this WebView: the app itself plus the
        // Nextcloud login hop. Everything else goes to the real browser.
        private val ALLOWED_HOSTS = setOf("life.xinutec.org", "dash.xinutec.org")
        private const val KEY_LAST_URL = "last_url"

        // Shop hosts the hidden fetch + connect overlay may load. Adding a shop
        // (e.g. "asda.com") is a one-line change here; everything else the shop
        // needs (URLs, consent, extraction) lives in the web app's provider.
        private val SHOP_HOSTS = setOf("waitrose.com", "asda.com")

        // Give up on a shop lookup after this long (bot-wall + SPA boot + fetch).
        private const val SHOP_TIMEOUT_MS = 20_000L

        // Injected at document start in the hidden shop view: patch fetch/XHR to
        // capture whatever Bearer token the SPA attaches to its own API calls, into
        // window.__authToken for the extractor to use. No regex (its '$' can't live
        // in a Kotlin const raw string) — case-insensitive string compare instead.
        private const val SHOP_CAPTURE_JS = """
            (function () {
              if (window.__shopCapInit) return; window.__shopCapInit = 1; window.__authToken = null;
              function isAuth(k) { return String(k).toLowerCase() === 'authorization'; }
              function ra(h) { if (!h) return null;
                if (typeof h.get === 'function') return h.get('authorization') || h.get('Authorization');
                if (Array.isArray(h)) { for (var i = 0; i < h.length; i++) { if (isAuth(h[i][0])) return h[i][1]; } return null; }
                for (var k in h) { if (isAuth(k)) return h[k]; } return null; }
              var of = window.fetch;
              window.fetch = function (u, o) { try { var a = ra(o && o.headers); if (a) window.__authToken = a; } catch (e) {} return of.apply(this, arguments); };
              var os = XMLHttpRequest.prototype.setRequestHeader;
              XMLHttpRequest.prototype.setRequestHeader = function (k, v) { try { if (isAuth(k)) window.__authToken = v; } catch (e) {} return os.apply(this, arguments); };
            })();
        """
    }
}
