package org.xinutec.life

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlarmManager
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Build
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
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.FrameLayout
import android.widget.TextView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

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

    // Whether we've already dropped Nextcloud's stale cookies for this launch (see
    // onReceivedHttpError). One shot: if the login still fails after a clean start,
    // the fault is not stale cookies and retrying would only spin.
    private var staleLoginRecovered = false

    // The explanation strip, if one is showing.
    private var banner: TextView? = null

    /** Explain, over the page, why the app is doing something the user didn't ask
     *  for. Native rather than injected: the page underneath at that moment belongs
     *  to Nextcloud, and we neither can nor should write into it. Dismiss on tap;
     *  otherwise it goes on its own once it's been read. */
    private fun showBanner(text: String) {
        banner?.let { root.removeView(it) }
        val strip =
            TextView(this).apply {
                setText(text)
                setPadding(BANNER_PAD, BANNER_PAD, BANNER_PAD, BANNER_PAD)
                setBackgroundColor(BANNER_BG)
                setTextColor(Color.WHITE)
                textSize = 14f
                layoutParams =
                    FrameLayout.LayoutParams(
                        FrameLayout.LayoutParams.MATCH_PARENT,
                        FrameLayout.LayoutParams.WRAP_CONTENT,
                        Gravity.TOP,
                    )
                setOnClickListener { hideBanner() }
            }
        banner = strip
        root.addView(strip)
        strip.postDelayed({ if (banner === strip) hideBanner() }, BANNER_MS)
    }

    private fun hideBanner() {
        banner?.let { root.removeView(it) }
        banner = null
    }

    /** Forget every Nextcloud cookie, so the next login starts from the cookie-less
     *  state that Nextcloud actually handles correctly. */
    private fun clearNextcloudCookies() {
        val cm = CookieManager.getInstance()
        val base = "https://$NC_HOST/"
        // The store holds HttpOnly cookies too (this is not JS), so the live names
        // come from it — the session cookie's name is instance-specific and can't be
        // hardcoded. The known fixed ones are cleared as well, in case the store
        // hands back nothing.
        val live =
            cm
                .getCookie(base)
                ?.split(";")
                ?.mapNotNull { it.substringBefore('=').trim().ifEmpty { null } }
                .orEmpty()
        for (name in live + NC_FIXED_COOKIES) {
            // No Domain attribute: __Host- prefixed cookies reject one, and the rest
            // are host-only anyway.
            cm.setCookie(base, "$name=; Max-Age=0; Path=/; Secure")
        }
        cm.flush()
    }

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
                // Reminders: the web app schedules device-local notifications
                // (e.g. the daily wellbeing check-in nudge) at a wall-clock time,
                // fired by AlarmManager → ReminderReceiver even when the app is
                // closed. Generic — nothing wellbeing-specific lives here; the web
                // app owns the "when", the copy, and the deep-link target.
                addJavascriptInterface(ReminderBridge(), "ReminderBridge")
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

                        // Recover from Nextcloud refusing the login with 403 "State
                        // token does not match".
                        //
                        // NC writes the login's state token into the session named by
                        // whatever session cookie you arrive with. This WebView keeps
                        // NC's cookies for months, while NC sweeps its sessions — so
                        // by the time we sign in again the cookie names a session the
                        // server has forgotten, the token dies with it, and the grant
                        // step refuses. A cookie-LESS browser skips the whole path
                        // (NC's same-site middleware only engages when cookies exist),
                        // which is why a fresh install works and a long-lived one does
                        // not. Reproduced in desktop Chrome too, so this is NC's
                        // behaviour, not a WebView quirk.
                        //
                        // Dropping NC's cookies and starting over is exactly the state
                        // that works. Once per launch, so a genuinely broken login
                        // can't loop.
                        override fun onReceivedHttpError(
                            view: WebView,
                            request: WebResourceRequest,
                            errorResponse: WebResourceResponse,
                        ) {
                            super.onReceivedHttpError(view, request, errorResponse)
                            if (!request.isForMainFrame) return
                            if (errorResponse.statusCode != HTTP_FORBIDDEN) return
                            if (request.url.host != NC_HOST || staleLoginRecovered) return
                            staleLoginRecovered = true
                            Log.w(
                                TAG,
                                "NC refused the login (403) — clearing its stale cookies and retrying",
                            )
                            // Don't let NC's "Access denied" page paint. We are about to
                            // fix it, and someone who sees that flash past has no way to
                            // tell whether anything is wrong or whether trying again is
                            // pointless.
                            view.stopLoading()
                            clearNextcloudCookies()
                            // And say WHY the login is being asked for twice. A silent
                            // retry still leaves you guessing: the recovery worked, but
                            // only the log knew it.
                            showBanner(
                                "Your Nextcloud sign-in had expired, so it was refused. " +
                                    "Cleared it — signing in again should work now.",
                            )
                            view.loadUrl("${LIFE_URL}login")
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
        // A notification tap launches us with a deep-link target; honour it over the
        // reopen-where-you-left-off default. Otherwise reopen the last in-app page
        // (the hardcoded URL is only the first-run default).
        web.loadUrl(
            reminderTargetUrl(intent) ?: prefs.getString(KEY_LAST_URL, null) ?: LIFE_URL,
        )
    }

    // A notification tapped while we're already running arrives here (SINGLE_TOP),
    // not through a fresh onCreate — navigate the live WebView to its target.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        reminderTargetUrl(intent)?.let { web.loadUrl(it) }
    }

    /** The in-app URL a reminder's notification wants opened, or null if this intent
     *  carries none. Confined to the life app: a relative path is resolved against
     *  [LIFE_URL], and an absolute URL is honoured only if it's already an app URL —
     *  a reminder can never point the WebView off-origin. */
    private fun reminderTargetUrl(intent: Intent?): String? {
        val raw = intent?.getStringExtra(EXTRA_OPEN_URL)?.trim().orEmpty()
        if (raw.isEmpty()) return null
        if (raw.startsWith(LIFE_URL)) return raw
        if ("://" in raw) return null // an off-origin absolute URL — refuse it
        return LIFE_URL.trimEnd('/') + "/" + raw.trimStart('/')
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
                // Full-size, not 1×1: a bot wall's JS challenge (Cloudflare) fingerprints
                // the render — a 1×1 viewport can fail it or its clearance redirect. The
                // view is added *behind* the visible app (index 0), so it renders like a
                // real browser tab yet the user never sees it.
                layoutParams =
                    FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT,
                    )
                settings.useWideViewPort = true
                settings.loadWithOverviewMode = true
                // Drop the WebView tells from the UA ("; wv" and "Version/4.0") so it
                // reads as ordinary mobile Chrome — bot walls treat WebViews harshly.
                settings.userAgentString =
                    settings.userAgentString.replace("; wv", "").replace("Version/4.0 ", "")
            }
        // A shop SPA's session/consent flow leans on third-party cookies, which a
        // WebView blocks by default.
        CookieManager.getInstance().setAcceptThirdPartyCookies(hidden, true)
        shopWeb = hidden
        root.addView(hidden, 0)

        val settled = AtomicBoolean(false)
        val retries = AtomicInteger(0)
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
                // A main-frame redirect to a non-http(s) scheme (an app deep link like
                // intent://, market://) would otherwise surface as a fatal main-frame
                // load error. Swallow those (there's nothing to open here) and log,
                // rather than killing the whole fetch on a stray redirect.
                override fun shouldOverrideUrlLoading(
                    view: WebView,
                    request: WebResourceRequest,
                ): Boolean {
                    val scheme = request.url.scheme
                    if (scheme != "https" && scheme != "http") {
                        Log.d("life-shop", "blocked non-http redirect: ${request.url}")
                        return true
                    }
                    return false
                }

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
                    Log.d(
                        "life-shop",
                        "onReceivedError main=${request.isForMainFrame} " +
                            "code=${error.errorCode} desc=${error.description} url=${request.url}",
                    )
                    if (!request.isForMainFrame) return
                    // The first fetch after a cold start can hit a transient DNS miss
                    // (ERR_NAME_NOT_RESOLVED): Chromium's resolver isn't ready until a
                    // few seconds after the process starts (the VPN advertises no DNS,
                    // so it has to fall back to the underlying network). It recovers on
                    // its own, so re-load on a fixed cadence until it does — the page is
                    // server-rendered, so a successful load extracts immediately. Bounded
                    // by SHOP_TIMEOUT_MS overall and MAX_SHOP_RETRIES here.
                    if (retries.getAndIncrement() < MAX_SHOP_RETRIES) {
                        Log.d("life-shop", "retrying main-frame load, attempt ${retries.get()}")
                        view.postDelayed({ if (!settled.get()) view.loadUrl(url) }, SHOP_RETRY_MS)
                        return
                    }
                    finish("""{"ok":false,"error":"load failed"}""")
                }

                // A bot wall answering 403/503 arrives here, not onReceivedError. Log
                // it (with the challenge status) so a wall is distinguishable from a
                // network failure; don't finish — the challenge page may still resolve.
                override fun onReceivedHttpError(
                    view: WebView,
                    request: WebResourceRequest,
                    errorResponse: android.webkit.WebResourceResponse,
                ) {
                    if (request.isForMainFrame) {
                        Log.d(
                            "life-shop",
                            "onReceivedHttpError status=${errorResponse.statusCode} url=${request.url}",
                        )
                    }
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
            // dev-lint: android-native-chrome allow — external login overlay
            Button(this).apply {
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

    /**
     * Bridge for the web app's reminders: schedule a device-local notification to
     * fire at a wall-clock time, or cancel one. Generic over `id` — a stable string
     * key the web app owns (e.g. "wellbeing-daily"); re-scheduling the same id
     * overwrites its pending alarm, so the web app re-arms idempotently on each open.
     * Alarms survive the app being closed but not a reboot — the web app re-arms
     * after one. Nothing wellbeing-specific lives here.
     */
    inner class ReminderBridge {
        @JavascriptInterface fun available(): Boolean = true

        /** Fire notification [title]/[body] at [whenMs] (epoch ms); tapping it opens
         *  the app at [url] (a path or an app URL). Re-scheduling [id] replaces it. */
        @JavascriptInterface
        fun schedule(id: String, whenMs: Double, title: String, body: String, url: String?) {
            ReminderReceiver.ensureChannel(this@MainActivity)
            ensureNotificationsAllowed()
            val intent =
                Intent(this@MainActivity, ReminderReceiver::class.java).apply {
                    putExtra(ReminderReceiver.EXTRA_ID, id)
                    putExtra(ReminderReceiver.EXTRA_TITLE, title)
                    putExtra(ReminderReceiver.EXTRA_BODY, body)
                    if (url != null) putExtra(ReminderReceiver.EXTRA_URL, url)
                }
            val am = getSystemService(AlarmManager::class.java)
            val at = whenMs.toLong()
            // Exact where allowed (USE_EXACT_ALARM is auto-granted for this sideloaded
            // app); fall back to an inexact idle-tolerant alarm if a future OS/policy
            // revokes it — a reminder should be late, never lost.
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || am.canScheduleExactAlarms()) {
                am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, broadcastFor(id, intent))
            } else {
                am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, broadcastFor(id, intent))
            }
        }

        /** Cancel a pending reminder and dismiss any notification it already posted. */
        @JavascriptInterface
        fun cancel(id: String) {
            val intent = Intent(this@MainActivity, ReminderReceiver::class.java)
            getSystemService(AlarmManager::class.java).cancel(broadcastFor(id, intent))
            getSystemService(NotificationManager::class.java).cancel(id.hashCode())
        }
    }

    /** A PendingIntent addressing the reminder receiver, keyed by the reminder id so a
     *  re-schedule updates in place and a cancel matches (extras don't affect match). */
    private fun broadcastFor(id: String, intent: Intent): PendingIntent =
        PendingIntent.getBroadcast(
            this,
            id.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    /** Ask for POST_NOTIFICATIONS (Android 13+) the first time a reminder is set, so
     *  the prompt is tied to turning a reminder on. Best-effort: the alarm is armed
     *  regardless; a declined grant just means the notification is suppressed. */
    private fun ensureNotificationsAllowed() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        runOnUiThread {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTIF_REQ)
        }
    }

    companion object {
        private const val CAMERA_REQ = 1
        private const val FILE_REQ = 2
        private const val NOTIF_REQ = 3

        /** Intent extra: the in-app URL/path a tapped reminder should open. */
        const val EXTRA_OPEN_URL = "open_url"

        // Skip pasting anything larger than the backend's 5 MiB image cap.
        private const val MAX_PASTE_BYTES = 5 * 1024 * 1024

        // The life app (HTTPS, behind a Nextcloud-identity login).
        private const val LIFE_URL = "https://life.xinutec.org/"

        // Hosts allowed to load inside this WebView: the app itself plus the
        // Nextcloud login hop. Everything else goes to the real browser.
        private const val NC_HOST = "dash.xinutec.org"
        private val ALLOWED_HOSTS = setOf("life.xinutec.org", NC_HOST)
        private const val KEY_LAST_URL = "last_url"

        private const val TAG = "life-app"
        private const val HTTP_FORBIDDEN = 403

        // The explanation strip: dark slate, readable over any page, gone after a
        // long-enough read (it says something the user needs, not a toast).
        private const val BANNER_PAD = 28
        private const val BANNER_BG = 0xEE1F2937.toInt()
        private const val BANNER_MS = 9_000L

        // Nextcloud's fixed cookie names. The session cookie's own name is derived
        // from the instance id, so it can't be listed here — it is read from the
        // cookie store instead (see clearNextcloudCookies).
        private val NC_FIXED_COOKIES =
            listOf(
                "oc_sessionPassphrase",
                "__Host-nc_sameSiteCookielax",
                "__Host-nc_sameSiteCookiestrict",
                "nc_username",
                "nc_token",
                "nc_session_id",
            )

        // Shop hosts the hidden fetch + connect overlay may load. Adding a shop
        // (e.g. "asda.com") is a one-line change here; everything else the shop
        // needs (URLs, consent, extraction) lives in the web app's provider.
        private val SHOP_HOSTS = setOf("waitrose.com", "asda.com")

        // Give up on a shop lookup after this long (bot-wall + SPA boot + fetch).
        private const val SHOP_TIMEOUT_MS = 45_000L

        // Cold-start DNS can be unavailable for several seconds; re-load on this cadence
        // until it settles (see onReceivedError).
        private const val SHOP_RETRY_MS = 2_000L
        private const val MAX_SHOP_RETRIES = 8

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
