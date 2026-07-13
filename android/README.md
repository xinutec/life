# life web viewer (Android)

The `life.xinutec.org` app (the personal home-OS) presented as a native-feeling
app: a single full-screen **WebView**, no address bar, no tabs, a home-screen icon.
It avoids browser chrome while showing the UI exactly as designed (the system
WebView is Chromium, so it renders like Chrome).

The site is **behind a login** (Nextcloud identity — a self-hosted IdP that works in
a WebView). The WebView keeps the session cookie, so it's a **one-time sign-in**; the
app needs only `INTERNET`.

## What it does

- Loads `https://life.xinutec.org/` — **hardcoded** (`MainActivity.LIFE_URL`); this
  app is single-purpose.
- JavaScript + DOM storage on (Angular), all navigation kept in-app, Back walks the
  SPA history.
- Insets the WebView from the system bars by padding a wrapper, and paints the
  strips behind the bars with the page's own surface colour (read on load, so it
  tracks the Material light/dark theme). The WebView no longer underlaps the bars,
  so the page's own `env(safe-area-inset-*)` collapse to 0 and add nothing on top.

Runs on any Android 8+ (minSdk 26) device.

## Build & install

The toolchain is this repo's own `android` dev shell (JDK 17 + Android SDK + adb;
the Gradle wrapper pins Gradle):

```sh
cd android
nix develop ..#android --command ./gradlew :app:assembleDebug
# → app/build/outputs/apk/debug/app-debug.apk
```

Or just `./deploy.sh`, which builds and installs to the Pixel 9, finding it by
*model* rather than IP (a Pixel 5 is often adb-connected too):

```sh
nix develop ..#android --command ./deploy.sh
```

The APK is signed with the auto-generated debug key — fine for sideloading, the
only distribution path.

## Debugging the WebView

`MainActivity` calls `WebView.setWebContentsDebuggingEnabled(true)`, so the live
page is inspectable over adb with the full Chrome DevTools protocol — console,
network, DOM. **Use this.** The alternative is inferring what the page did from
server logs and screenshots, which is how a Nextcloud login error once took an
hour to identify instead of a minute (2026-07-13).

```sh
nix develop ..#android
adb connect 10.100.0.12:5555                       # VPN IP; LAN is 192.168.1.133
PID=$(adb -s 10.100.0.12:5555 shell pidof org.xinutec.life)
adb -s 10.100.0.12:5555 forward tcp:9333 localabstract:webview_devtools_remote_$PID
curl -s http://127.0.0.1:9333/json/list            # pages + their webSocketDebuggerUrl
```

Then drive it over CDP on that WebSocket, or open `chrome://inspect` in a desktop
Chrome. Plain `adb logcat` also carries the page's `console.*` output under the
`chromium` tag, which is enough when you only need the errors.

## Layout

```
android/
├── app/
│   ├── build.gradle.kts                          # android app module, no Compose/AppCompat
│   └── src/main/
│       ├── AndroidManifest.xml                   # INTERNET; single launcher activity
│       ├── kotlin/org/xinutec/life/MainActivity.kt    # the WebView
│       └── res/                                  # launcher icon (indigo dashboard), theme, strings
├── build.gradle.kts · settings.gradle.kts · gradle/   # project scaffolding
├── deploy.sh                                     # build + install to the Pixel 9 (by model)
└── gradlew                                       # SDK comes from ../flake.nix `.#android`
```
