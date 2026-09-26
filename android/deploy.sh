#!/usr/bin/env bash
# Build the APK and install it on the Pixel 9 over Wi-Fi, from android/:
#
#   nix develop ..#android --command ./deploy.sh [<ip[:port]>]
#
# Installs by serial after checking the model, never a bare `adb install`:
# the IP drifts and other phones are often connected.
set -euo pipefail
cd "$(dirname "$0")"

ADB="$ANDROID_HOME/platform-tools/adb"

echo "building APK…"
./gradlew :app:assembleDebug -q
APK="$PWD/app/build/outputs/apk/debug/app-debug.apk"

# Endpoints to try, in order. :5555 (persistent `adb tcpip`) survives sleep, so try
# it first — VPN IP (stable, 10.100.0.12) then the LAN DHCP reservation. Override
# with an arg if wireless debugging rotated to a random port.
CANDIDATES=("${1:-}" "10.100.0.12:5555" "192.168.1.133:5555")

for EP in "${CANDIDATES[@]}"; do
  [ -z "$EP" ] && continue
  [[ "$EP" == *:* ]] || EP="$EP:5555"
  "$ADB" connect "$EP" 2>&1 | grep -qiE "connected|already" || continue
  MODEL="$("$ADB" -s "$EP" shell getprop ro.product.model 2>/dev/null | tr -d '\r')"
  if [ "$MODEL" != "Pixel 9" ]; then
    echo "  skip $EP — reports model '$MODEL', not 'Pixel 9'." >&2
    continue
  fi
  echo "=== installing to Pixel 9 ($EP) ==="
  "$ADB" -s "$EP" install -r "$APK"
  "$ADB" -s "$EP" shell am start -S -n org.xinutec.life/.MainActivity >/dev/null
  echo "  installed + launched on Pixel 9 ($EP)."
  exit 0
done

echo "Pixel 9 not reachable on :5555 (VPN or LAN). Re-enable wireless debugging or" >&2
echo "re-run 'adb tcpip 5555', then pass the ip:port as an argument." >&2
exit 1
