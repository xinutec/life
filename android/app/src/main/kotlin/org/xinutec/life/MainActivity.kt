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
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject

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

    // The offscreen WebView doing a Waitrose lookup, if one is in flight. One at a
    // time; a new request tears down the previous.
    private var waitroseWeb: WebView? = null

    // The visible "Connect Waitrose" login overlay + its WebView, while shown.
    private var connectOverlay: FrameLayout? = null
    private var connectWeb: WebView? = null

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
                // Waitrose enrichment: the web app asks us to fetch a product by
                // its lineNumber; we do it in a hidden WebView on waitrose.com (a
                // real browser passes the Akamai bot wall a server-side client
                // can't) and hand the normalized product back. See WaitroseBridge.
                addJavascriptInterface(WaitroseBridge(), "WaitroseBridge")
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
                            Log.d("life-web", "${msg.message()} (${msg.sourceId()}:${msg.lineNumber()})")
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
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
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
        waitroseWeb?.let { root.removeView(it); it.destroy() }
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
            if (cw != null && cw.canGoBack()) cw.goBack() else closeWaitroseConnect()
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
     * Bridge for the web app's Waitrose enrichment. `fetchProduct(lineNumber, id)`
     * starts a lookup and resolves asynchronously by calling
     * `window.__waitroseResolve(id, { ok, product | error })` back in the life app.
     * `available()` lets the web app feature-detect (the object is absent in a
     * plain browser). Origin-gated in waitroseFetchProduct.
     */
    inner class WaitroseBridge {
        @JavascriptInterface fun available(): Boolean = true

        @JavascriptInterface
        fun fetchProduct(lineNumber: String, requestId: String) {
            runOnUiThread { waitroseFetchProduct(lineNumber, requestId) }
        }

        // Show the visible Waitrose login so a session is established (cookies
        // persist in the shared jar; the hidden fetch needs a logged-in SPA to
        // mint a token). Resolves nothing — the web app hears back via
        // window.__waitroseConnected() when the overlay closes.
        @JavascriptInterface
        fun connect() {
            runOnUiThread { showWaitroseConnect() }
        }
    }

    /**
     * Fetch a Waitrose product by lineNumber in a throwaway offscreen WebView.
     * Why a WebView, not an HTTP client: waitrose.com is behind Akamai Bot Manager,
     * which rejects non-browser TLS/HTTP2 fingerprints outright — only a real
     * browser engine passes. The hidden view loads a waitrose.com page (the SPA
     * mints its session Bearer token), we capture that token by patching fetch/XHR,
     * then call the JSON product API with it. The result returns through the
     * per-view AndroidWtr bridge because WebView.evaluateJavascript does NOT await
     * promises (unlike CDP).
     */
    @SuppressLint("SetJavaScriptEnabled")
    private fun waitroseFetchProduct(lineNumber: String, requestId: String) {
        if (web.url?.startsWith(LIFE_URL) != true) {
            resolveWaitrose(requestId, """{"ok":false,"error":"forbidden"}""")
            return
        }
        // lineNumber is spliced into the URL below — allow only digits.
        if (lineNumber.isEmpty() || lineNumber.length > 10 || !lineNumber.all(Char::isDigit)) {
            resolveWaitrose(requestId, """{"ok":false,"error":"bad lineNumber"}""")
            return
        }

        waitroseWeb?.let { root.removeView(it); it.destroy() }
        val hidden =
            WebView(this).apply {
                settings.javaScriptEnabled = true
                settings.domStorageEnabled = true
                // Offscreen but attached (1×1) so JS timers/network run reliably.
                layoutParams = FrameLayout.LayoutParams(1, 1)
            }
        // The Waitrose SPA's session/consent flow leans on third-party cookies,
        // which a WebView blocks by default.
        CookieManager.getInstance().setAcceptThirdPartyCookies(hidden, true)
        waitroseWeb = hidden
        root.addView(hidden)

        val settled = AtomicBoolean(false)
        val finish = { payload: String ->
            if (settled.compareAndSet(false, true)) {
                resolveWaitrose(requestId, payload)
                hidden.post {
                    root.removeView(hidden)
                    hidden.destroy()
                    if (waitroseWeb === hidden) waitroseWeb = null
                }
            }
        }
        hidden.addJavascriptInterface(
            object {
                @JavascriptInterface fun result(json: String) = runOnUiThread { finish(json) }
            },
            "AndroidWtr",
        )
        hidden.webChromeClient =
            object : WebChromeClient() {
                override fun onConsoleMessage(msg: ConsoleMessage): Boolean {
                    Log.d("life-wtr", "${msg.message()} (${msg.sourceId()}:${msg.lineNumber()})")
                    return true
                }
            }
        hidden.webViewClient =
            object : WebViewClient() {
                // Patch fetch/XHR early, before the SPA fires its authed calls, so
                // we catch the Bearer token it attaches.
                override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                    view.evaluateJavascript(WAITROSE_CAPTURE_JS, null)
                }

                override fun onPageFinished(view: WebView, url: String) {
                    view.evaluateJavascript(waitroseFetchJs(lineNumber), null)
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
        hidden.postDelayed({ finish("""{"ok":false,"error":"timeout"}""") }, WAITROSE_TIMEOUT_MS)
        // A search page for the lineNumber reliably fires an authenticated API call
        // (so the token is minted + captured); its results are ignored.
        hidden.loadUrl("https://www.waitrose.com/ecom/shop/search?searchTerm=$lineNumber")
    }

    /** Resolve the web app's pending promise with a JSON result object. */
    private fun resolveWaitrose(requestId: String, resultJson: String) {
        val js =
            "window.__waitroseResolve && window.__waitroseResolve(${JSONObject.quote(requestId)}, $resultJson)"
        web.post { web.evaluateJavascript(js, null) }
    }

    /**
     * Show a full-screen Waitrose WebView so the user signs in once. The session
     * cookies land in the shared CookieManager, so the hidden fetch view (and any
     * future basket/order calls) inherit a logged-in session. A "Done" button and
     * the back key close it; the web app is notified via window.__waitroseConnected().
     */
    @SuppressLint("SetJavaScriptEnabled")
    private fun showWaitroseConnect() {
        if (connectOverlay != null) return // already open
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
                            Log.d("life-wtr", "connect: ${msg.message()}")
                            return true
                        }
                    }
                webViewClient =
                    object : WebViewClient() {
                        // Keep waitrose.com (and its subdomains) in this view; hand
                        // anything else to the real browser.
                        override fun shouldOverrideUrlLoading(
                            view: WebView,
                            request: WebResourceRequest,
                        ): Boolean {
                            val host = request.url.host ?: return false
                            if (request.url.scheme == "https" &&
                                (host == "waitrose.com" || host.endsWith(".waitrose.com"))
                            ) {
                                return false
                            }
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

        val done =
            Button(this).apply {
                text = "Done"
                setOnClickListener { closeWaitroseConnect() }
                layoutParams =
                    FrameLayout.LayoutParams(
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
        cw.loadUrl(WAITROSE_URL)
    }

    private fun closeWaitroseConnect() {
        val overlay = connectOverlay ?: return
        connectOverlay = null
        root.removeView(overlay)
        connectWeb?.destroy()
        connectWeb = null
        // Let the web app re-check / retry the fetch now a session may exist.
        web.evaluateJavascript("window.__waitroseConnected && window.__waitroseConnected()", null)
    }

    // The in-page fetch: poll for the captured Bearer token, then call the product
    // API and report a normalized product (or an error) back through AndroidWtr.
    // lineNumber is digits-only (validated by the caller), safe to interpolate.
    private fun waitroseFetchJs(lineNumber: String): String =
        """
        (async () => {
          function log(m) { try { console.log('[wtr] ' + m); } catch (e) {} }
          // Dismiss the cookie-consent banner a fresh WebView shows ("Allow all");
          // once accepted the consent cookie persists in the shared jar, so later
          // fetches skip this. The SPA gates its authed API calls on consent, so
          // without this no token is ever minted.
          function clickAccept() {
            var b = document.querySelector('.acceptAll');
            if (!b) b = Array.prototype.find.call(document.querySelectorAll('button'),
              function (x) { return /allow all|accept all/i.test(x.innerText || ''); });
            if (b) { b.click(); return true; }
            return false;
          }
          try {
            for (var c = 0; c < 20 && !clickAccept(); c++) await new Promise(function (r) { setTimeout(r, 150); });
            log('consent handled');
            for (var i = 0; i < 40 && !window.__wtrCap; i++) await new Promise(function (r) { setTimeout(r, 250); });
            var tok = window.__wtrCap;
            if (!tok) { log('no token'); AndroidWtr.result(JSON.stringify({ ok: false, error: "no token" })); return; }
            log('got token');
            var r = await fetch("https://www.waitrose.com/api/products-prod/v1/products/$lineNumber?view=SUMMARY",
              { headers: { accept: "application/json", authorization: tok }, credentials: "include" });
            if (r.status !== 200) { AndroidWtr.result(JSON.stringify({ ok: false, error: "status " + r.status })); return; }
            var j = await r.json();
            var p = (j.products && j.products[0]) || null;
            if (!p) { AndroidWtr.result(JSON.stringify({ ok: false, error: "not found" })); return; }
            var im = p.images || {};
            var pr = p.pricing || {};
            var out = { ok: true, product: {
              source: "waitrose", external_id: p.lineNumber, name: p.name || null, brand: p.brand || null,
              barcodes: p.barCodes || [], image_url: im.large || im.medium || im.extraLarge || im.small || null,
              display_price: (pr.currentSaleUnitRetailPrice && pr.currentSaleUnitRetailPrice.price) || null,
              categories: (p.categories || []).map(function (c) { return c.name; })
            } };
            AndroidWtr.result(JSON.stringify(out));
          } catch (e) { AndroidWtr.result(JSON.stringify({ ok: false, error: String(e) })); }
        })();
        """.trimIndent()

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

        // The Waitrose site (for the visible "Connect Waitrose" sign-in overlay).
        private const val WAITROSE_URL = "https://www.waitrose.com/"

        // Give up on a Waitrose lookup after this long (Akamai + SPA boot + fetch).
        private const val WAITROSE_TIMEOUT_MS = 20_000L

        // Injected at document start in the hidden Waitrose view: patch fetch/XHR to
        // capture the Bearer token the SPA attaches to its own API calls. No regex
        // (its '$' can't live in a Kotlin const raw string) — case-insensitive
        // string compare instead.
        private const val WAITROSE_CAPTURE_JS = """
            (function () {
              if (window.__wtrCapInit) return; window.__wtrCapInit = 1; window.__wtrCap = null;
              function isAuth(k) { return String(k).toLowerCase() === 'authorization'; }
              function ra(h) { if (!h) return null;
                if (typeof h.get === 'function') return h.get('authorization') || h.get('Authorization');
                if (Array.isArray(h)) { for (var i = 0; i < h.length; i++) { if (isAuth(h[i][0])) return h[i][1]; } return null; }
                for (var k in h) { if (isAuth(k)) return h[k]; } return null; }
              var of = window.fetch;
              window.fetch = function (u, o) { try { var a = ra(o && o.headers); if (a) window.__wtrCap = a; } catch (e) {} return of.apply(this, arguments); };
              var os = XMLHttpRequest.prototype.setRequestHeader;
              XMLHttpRequest.prototype.setRequestHeader = function (k, v) { try { if (isAuth(k)) window.__wtrCap = v; } catch (e) {} return os.apply(this, arguments); };
            })();
        """
    }
}
