package org.xinutec.life

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.util.Base64
import android.util.Log
import android.view.Gravity
import android.webkit.CookieManager
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject
import org.xinutec.shell.ShellConfig
import org.xinutec.shell.WebDebugging
import org.xinutec.shell.WebShellActivity
import org.xinutec.shell.sameOrigin

/**
 * life's Angular SPA at [LIFE_URL] in the fleet's [WebShellActivity]; the
 * WebView keeps the Nextcloud session cookie. Adds the clipboard, shop and
 * reminder bridges, the file chooser and camera grant, and recovery from a
 * stale-cookie login refusal.
 */
class MainActivity : WebShellActivity() {
    override val shell =
        ShellConfig(
            url = LIFE_URL,
            // The app itself plus the Nextcloud login hop; everything else goes to
            // the real browser.
            allowedHosts = setOf("life.xinutec.org", NC_HOST),
            consoleTag = "life-web",
            // Make both WebViews inspectable over adb (chrome://inspect / CDP). The
            // app is a personal, sideloaded debug build; this is how in-app web + the
            // hidden Waitrose fetch get diagnosed (the view isn't otherwise
            // remote-debuggable).
            webDebugging = WebDebugging.ALWAYS,
        )

    // A pending web camera request, held while the OS permission dialog is up.
    private var pendingCameraRequest: PermissionRequest? = null

    // A pending <input type=file> result callback, held while the picker is open.
    private var fileChooserCallback: ValueCallback<Array<Uri>>? = null

    private val shop by lazy { ShopBridge(this, root, web, ::syncBack) }

    // Whether we've already dropped Nextcloud's stale cookies for this launch (see
    // onReceivedHttpError). One shot: if the login still fails after a clean start,
    // the fault is not stale cookies and retrying would only spin.
    private var staleLoginRecovered = false

    // The explanation strip, if one is showing.
    private var banner: TextView? = null

    /**
     * The native capabilities the web app drives, exposed only to [ALLOWED_ORIGINS].
     *
     * `addWebMessageListener` injects per origin, and each listener also checks
     * `sourceOrigin` and `isMainFrame`; `addJavascriptInterface` would reach every
     * frame, and the shop bridge runs caller-supplied JavaScript. Without
     * [WebViewFeature.WEB_MESSAGE_LISTENER] the bridges are absent and the web app
     * falls back to browser behaviour; never fall back to `addJavascriptInterface`.
     */
    override fun onWebViewCreated(web: WebView) {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) return
        // The system clipboard's image, for the web app's "Paste copied image"
        // action (e.g. an image copied in Chrome). A WebView can't read a clipboard
        // image itself, so the page asks us.
        listen(web, "AndroidClipboard", ::onClipboardMessage)
        // Shop enrichment: the web app drives a hidden WebView on a shop site (a
        // real browser passes the bot wall a server-side client can't) to fetch
        // product data, supplying the shop-specific URLs + extractor JS. Nothing
        // shop-specific lives here.
        listen(web, "ShopBridge") { body, _ -> onShopMessage(body) }
        // Reminders: the web app schedules device-local notifications (e.g. the
        // daily wellbeing check-in nudge) at a wall-clock time, fired by
        // AlarmManager → ReminderReceiver even when the app is closed. Generic —
        // the web app owns the "when", the copy, and the deep-link target.
        listen(web, "ReminderBridge") { body, _ -> onReminderMessage(body) }
    }

    /** Register [name] for the app's own origin, refusing anything else before the
     *  handler ever sees it. */
    private fun listen(
        web: WebView,
        name: String,
        handle: (JSONObject, JavaScriptReplyProxy) -> Unit,
    ) {
        WebViewCompat.addWebMessageListener(
            web,
            name,
            ALLOWED_ORIGINS,
        ) { _, message, origin, isMainFrame, proxy ->
            // The origin rules already did this; checked again so the bridge does not
            // depend on one line elsewhere staying right.
            if (isMainFrame && sameOrigin(LIFE_ORIGIN, origin.toString())) {
                val body = message.data?.let { runCatching { JSONObject(it) }.getOrNull() }
                if (body != null) handle(body, proxy)
            }
        }
    }

    /** `{op:"readImage"}` → the clipboard image as a `data:` URL, or `""` when
     *  there isn't one. Read on the UI thread, which is where this already runs. */
    private fun onClipboardMessage(body: JSONObject, proxy: JavaScriptReplyProxy) {
        if (body.optString("op") != "readImage") return
        proxy.postMessage(readClipboardImageDataUrl() ?: "")
    }

    /** `{op:"run", url, extractorJs, requestId}` / `{op:"connect", loginUrl,
     *  requestId}`. Both answer through `window.__shopResolve` /
     *  `window.__shopConnected` as they always did — a hidden WebView's result
     *  arrives long after the message that asked for it. So no reply proxy: a
     *  handler that never answers on the message channel does not hold one. */
    private fun onShopMessage(body: JSONObject) {
        val requestId = body.optString("requestId")
        when (body.optString("op")) {
            "run" -> {
                shop.run(body.optString("url"), body.optString("extractorJs"), requestId)
            }

            "connect" -> {
                shop.connect(body.optString("loginUrl"), requestId)
            }
        }
    }

    /** `{op:"schedule", id, whenMs, title, body, url}` / `{op:"cancel", id}`. */
    private fun onReminderMessage(body: JSONObject) {
        val id = body.optString("id")
        if (id.isEmpty()) return
        when (body.optString("op")) {
            "schedule" -> {
                scheduleReminder(
                    id = id,
                    whenMs = body.optDouble("whenMs"),
                    title = body.optString("title"),
                    text = body.optString("body"),
                    url = body.optString("url").ifEmpty { null },
                )
            }

            "cancel" -> {
                cancelReminder(id)
            }
        }
    }

    override fun createWebViewClient() = LifeWebViewClient()

    override fun createWebChromeClient() = LifeWebChromeClient()

    inner class LifeWebViewClient : ShellWebViewClient() {
        // Recover from Nextcloud's 403 "State token does not match". NC stores the
        // login's state token in the session named by the incoming cookie; this
        // WebView keeps NC cookies for months after NC has swept the session, so
        // the token is lost. Without cookies NC skips that path (desktop Chrome
        // behaves the same), so drop NC's cookies and start over, once per launch
        // so a broken login can't loop.
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
            Log.w(TAG, "NC refused the login (403) — clearing its stale cookies and retrying")
            // Don't let NC's "Access denied" page paint. We are about to fix it, and
            // someone who sees that flash past has no way to tell whether anything
            // is wrong or whether trying again is pointless.
            view.stopLoading()
            clearNextcloudCookies()
            // And say WHY the login is being asked for twice. A silent retry still
            // leaves you guessing: the recovery worked, but only the log knew it.
            showBanner(
                "Your Nextcloud sign-in had expired, so it was refused. " +
                    "Cleared it — signing in again should work now.",
            )
            view.loadUrl("${LIFE_URL}login")
        }
    }

    inner class LifeWebChromeClient : ShellWebChromeClient() {
        // The barcode scanner calls getUserMedia; a WebView denies camera access
        // unless we explicitly grant it. Grant video capture, asking the OS for the
        // runtime CAMERA permission first if we lack it.
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

        // A WebView ignores <input type=file> unless we launch the picker ourselves
        // and hand the chosen URIs back — without this, tapping the app's image
        // picker does nothing (it works in Chrome, which supplies its own file
        // dialog). The intent from createIntent() honours the input's `accept`
        // (image/*) and `multiple`, so it opens straight to the photo picker.
        override fun onShowFileChooser(
            webView: WebView,
            filePathCallback: ValueCallback<Array<Uri>>,
            fileChooserParams: FileChooserParams,
        ): Boolean {
            // Abandon any earlier pick that never resolved.
            fileChooserCallback?.onReceiveValue(null)
            fileChooserCallback = filePathCallback
            return try {
                filePicker.launch(fileChooserParams.createIntent())
                true
            } catch (_: ActivityNotFoundException) {
                fileChooserCallback = null
                false // let the WebView know no chooser was shown
            }
        }
    }

    /** The in-app URL a reminder's notification wants opened, or null if this intent
     *  carries none. Confined to the life app: a relative path is resolved against
     *  [LIFE_URL], and an absolute URL is honoured only if it's already an app URL —
     *  a reminder can never point the WebView off-origin. */
    override fun startUrl(intent: Intent?): String? {
        val raw = intent?.getStringExtra(EXTRA_OPEN_URL)?.trim().orEmpty()
        if (raw.isEmpty()) return null
        if (raw.startsWith(LIFE_URL)) return raw
        if ("://" in raw) return null // an off-origin absolute URL — refuse it
        return LIFE_URL.trimEnd('/') + "/" + raw.trimStart('/')
    }

    // The Waitrose connect overlay swallows back first: walk its history, then
    // close it, before the main app's back behaviour.
    override fun onBackBeforeHistory(): Boolean = shop.back()

    // While the overlay is up, back belongs to us even at the SPA's root.
    override fun hasExtraBackTargets(): Boolean = shop.hasBackTarget

    // The shell releases the main WebView; the ones this app made are ours.
    override fun onDestroy() {
        shop.destroy()
        super.onDestroy()
    }

    // Deliver the picked image URIs back to the waiting <input type=file>. The
    // callback MUST be answered even on cancel, or the input stays blocked and
    // won't reopen the picker on the next tap — a cancelled pick still arrives
    // here, and parseResult turns it into the null the WebView is waiting for.
    private val filePicker =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val callback = fileChooserCallback ?: return@registerForActivityResult
            fileChooserCallback = null
            callback.onReceiveValue(
                WebChromeClient.FileChooserParams.parseResult(result.resultCode, result.data),
            )
        }

    // Resolve the held web camera request once the user answers the OS dialog.
    // Still the request-code API rather than the Activity Result one: the request
    // it answers comes from the WebView's onPermissionRequest, which is not a
    // launcher call, so there is no contract to register against.
    @Deprecated("Deprecated in Java")
    @Suppress("DEPRECATION") // answered from onPermissionRequest, not a launcher (above)
    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<String>,
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

    private fun hasCameraPermission() =
        checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

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

    /**
     * The image on the system clipboard as a `data:` URL, or null if there isn't
     * one. A WebView can't read a clipboard image itself, so the page asks us.
     *
     * Called on the UI thread, which `ClipboardManager` requires, and only for the
     * app's own origin.
     */
    private fun readClipboardImageDataUrl(): String? {
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
     * Fire notification [title]/[text] at [whenMs] (epoch ms); tapping it opens
     * [url]. Re-scheduling the same [id] replaces its alarm, so the web app
     * re-arms idempotently on each open, which also covers reboots.
     */
    private fun scheduleReminder(
        id: String,
        whenMs: Double,
        title: String,
        text: String,
        url: String?,
    ) {
        ReminderReceiver.ensureChannel(this)
        ensureNotificationsAllowed()
        val intent =
            Intent(this, ReminderReceiver::class.java).apply {
                putExtra(ReminderReceiver.EXTRA_ID, id)
                putExtra(ReminderReceiver.EXTRA_TITLE, title)
                putExtra(ReminderReceiver.EXTRA_BODY, text)
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
    private fun cancelReminder(id: String) {
        val intent = Intent(this, ReminderReceiver::class.java)
        getSystemService(AlarmManager::class.java).cancel(broadcastFor(id, intent))
        getSystemService(NotificationManager::class.java).cancel(id.hashCode())
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
        private const val NOTIF_REQ = 3

        /** Intent extra: the in-app URL/path a tapped reminder should open. */
        const val EXTRA_OPEN_URL = "open_url"

        // Skip pasting anything larger than the backend's 5 MiB image cap.
        private const val MAX_PASTE_BYTES = 5 * 1024 * 1024

        // The life app (HTTPS, behind a Nextcloud-identity login).
        private const val LIFE_URL = "https://life.xinutec.org/"

        /** The same app as an **origin** — `scheme://host[:port]`, no trailing
         *  slash, which is the only form `addWebMessageListener` accepts as a rule
         *  (a malformed one throws when the WebView is built). Derived from
         *  [LIFE_URL] rather than written twice, so the bridges cannot end up
         *  scoped to a different place than the app loads. */
        private val LIFE_ORIGIN = LIFE_URL.trimEnd('/')

        /** The only origin the native bridges are injected into. */
        private val ALLOWED_ORIGINS = setOf(LIFE_ORIGIN)

        // The Nextcloud login hop.
        private const val NC_HOST = "dash.xinutec.org"

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
    }
}
