package org.xinutec.life

import android.annotation.SuppressLint
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.util.Log
import android.view.Gravity
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.FrameLayout
import org.json.JSONObject
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * The shop bridge: runs the web app's extractors against shop pages in a
 * hidden WebView, and shows a shop's login in a visible overlay. Answers go to
 * the app's [web] view through `window.__shopResolve` / `__shopConnected`.
 */
internal class ShopBridge(
    private val activity: Activity,
    private val root: FrameLayout,
    private val web: WebView,
    private val syncBack: () -> Unit,
) {
    // The offscreen WebView doing a shop lookup, if one is in flight. One at a
    // time; a new request tears down the previous.
    private var shopWeb: WebView? = null

    // The visible shop-login overlay + its WebView, and the pending connect
    // request to answer when it closes.
    private var connectOverlay: FrameLayout? = null
    private var connectWeb: WebView? = null
    private var connectRequestId: String? = null

    /** Whether the login overlay is up; while it is, back belongs to it. */
    val hasBackTarget: Boolean get() = connectOverlay != null

    /** Back inside the overlay: its own history first, then close it. */
    fun back(): Boolean {
        val cw = connectWeb
        if (connectOverlay == null) return false
        if (cw != null && cw.canGoBack()) cw.goBack() else closeConnect()
        return true
    }

    fun destroy() {
        shopWeb?.let {
            root.removeView(it)
            it.destroy()
        }
        connectOverlay?.let { root.removeView(it) }
        connectWeb?.destroy()
    }

    /**
     * Load a shop page in a throwaway offscreen WebView and run the web app's
     * [extractorJs] when it finishes; bot managers reject non-browser clients. A
     * capture patch exposes any Bearer the page attaches (window.__authToken).
     * Results come back through the per-view AndroidShop bridge, because
     * evaluateJavascript doesn't await promises. One at a time.
     */
    @SuppressLint("SetJavaScriptEnabled") // the WebView runs the app's own bundle
    fun run(url: String, extractorJs: String, requestId: String) {
        // No origin gate here: the web-message listener already refused anything
        // that isn't the app's own main frame.
        if (!isShopUrl(url)) {
            resolve(requestId, """{"ok":false,"error":"host not allowed"}""")
            return
        }
        shopWeb?.let {
            root.removeView(it)
            it.destroy()
        }
        val hidden =
            WebView(activity).apply {
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
                resolve(requestId, payload)
                hidden.post {
                    root.removeView(hidden)
                    hidden.destroy()
                    if (shopWeb === hidden) shopWeb = null
                }
            }
        }
        hidden.addJavascriptInterface(
            object {
                @JavascriptInterface
                fun result(json: String) = activity.runOnUiThread { finish(json) }
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
    private fun resolve(requestId: String, resultJson: String) {
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
    @SuppressLint("SetJavaScriptEnabled") // the WebView runs the app's own bundle
    fun connect(loginUrl: String, requestId: String) {
        if (connectOverlay != null) return // already open
        if (!isShopUrl(loginUrl)) {
            notifyConnected(requestId)
            return
        }
        connectRequestId = requestId
        val cw =
            WebView(activity).apply {
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
                                activity.startActivity(Intent(Intent.ACTION_VIEW, request.url))
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
            // dev-lint: allow-native-chrome — external login overlay
            Button(activity).apply {
                text = "Done"
                setOnClickListener { closeConnect() }
                layoutParams =
                    FrameLayout
                        .LayoutParams(
                            ViewGroup.LayoutParams.WRAP_CONTENT,
                            ViewGroup.LayoutParams.WRAP_CONTENT,
                        ).apply { gravity = Gravity.TOP or Gravity.END }
            }
        val overlay =
            FrameLayout(activity).apply {
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
        // Back now belongs to the overlay, even at the SPA's root.
        syncBack()
        cw.loadUrl(loginUrl)
    }

    private fun closeConnect() {
        val overlay = connectOverlay ?: return
        connectOverlay = null
        root.removeView(overlay)
        connectWeb?.destroy()
        connectWeb = null
        syncBack()
        val id = connectRequestId
        connectRequestId = null
        // Let the web app re-check / retry now a session may exist.
        notifyConnected(id)
    }

    /** Notify the web app that a connect overlay closed (requestId may be null). */
    private fun notifyConnected(requestId: String?) {
        val arg = requestId?.let { JSONObject.quote(it) } ?: "null"
        web.evaluateJavascript("window.__shopConnected && window.__shopConnected($arg)", null)
    }

    private companion object {
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
