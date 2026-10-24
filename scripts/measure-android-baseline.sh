#!/usr/bin/env bash
# Measures the Android baseline the guide asks for before the Kotlin rewrite
# (§14 step 0 / §16): cold start, foreground idle CPU, RSS/PSS, and the app
# uid's network byte counters.
#
#   scripts/measure-android-baseline.sh            # ~12 min (default)
#   scripts/measure-android-baseline.sh --quick    # ~1 min smoke
#
# Run it against the CURRENT build before the rewrite and again after cutover,
# then compare the two reports. Requires one device (`adb devices`).
#
# Caveats (record them with the numbers):
#   * With no root we cannot drop the page cache, so only the FIRST cold start
#     is close to a true cold start; later ones are warm.
#   * "Idle" means the app is foreground and untouched — the background sync
#     driver (10 s interval) and the one-shot update check are part of the
#     measurement, by design.
#   * Request *counts* are not observable without packet capture; the uid byte
#     counters are the honest proxy, and the per-action expectations (calendar
#     open = 0 requests, one update check per process) are asserted by tests.
set -euo pipefail

PKG="${PKG:-com.kal.calendar}"
QUICK=0
[ "${1:-}" = "--quick" ] && QUICK=1
SAMPLES=$([ "$QUICK" = 1 ] && echo 2 || echo 10)
INTERVAL=$([ "$QUICK" = 1 ] && echo 15 || echo 60)

DEVICES="$(adb devices | awk 'NR>1 && $2=="device" {print $1}')"
[ -n "$DEVICES" ] || { echo "no device attached (adb devices)" >&2; exit 1; }
ACT="$(adb shell cmd package resolve-activity --brief "$PKG" | tail -1 | tr -d '\r')"
[ -n "$ACT" ] || { echo "cannot resolve the launcher activity for $PKG" >&2; exit 1; }

echo "# Kal Android baseline"
echo "package:  $PKG"
echo "activity: $ACT"
echo "device:   $(adb shell getprop ro.product.model | tr -d '\r') / Android $(adb shell getprop ro.build.version.release | tr -d '\r')"
echo "build:    $(adb shell dumpsys package "$PKG" | grep -m1 versionName | tr -d '\r') $(adb shell dumpsys package "$PKG" | grep -m1 versionCode | tr -d '\r')"
echo "date:     $(date -Is)"
echo
echo "## Cold start (force-stop, then am start -W)"
for i in $(seq 1 3); do
  adb shell am force-stop "$PKG"
  sleep 2
  printf 'run %s: ' "$i"
  adb shell am start -W -n "$ACT" 2>&1 | grep -E "TotalTime" | tr -d '\r' || echo "failed"
  adb shell am force-stop "$PKG"
  sleep 1
done

echo
echo "## Foreground idle (samples every ${INTERVAL}s)"
adb shell am start -n "$ACT" >/dev/null
sleep 20   # let first paint + the initial update check settle

PID="$(adb shell pidof "$PKG" | tr -d '\r' | awk '{print $1}')"
[ -n "$PID" ] || { echo "app not running after launch" >&2; exit 1; }
# The app uid for the network counters: /proc/<pid>/status is the reliable
# source (`dumpsys package` does not print a userId= line on Android 14).
UID_="$(adb shell cat "/proc/$PID/status" 2>/dev/null | awk '/^Uid:/ {print $2}' | tr -d '\r')"
[ -n "$UID_" ] || UID_=0

# utime+stime from /proc/<pid>/stat (fields 14/15; the comm field has no spaces).
cpu_ticks() { adb shell cat "/proc/$PID/stat" 2>/dev/null | awk '{print $14+$15}'; }
rss_kb() { adb shell cat "/proc/$PID/status" 2>/dev/null | awk '/VmRSS/ {print $2}'; }
pss_kb() { adb shell dumpsys meminfo "$PKG" 2>/dev/null | awk '/TOTAL PSS/ {print $3}'; }
net_bytes() { adb shell dumpsys netstats detail 2>/dev/null | tr -d '\r' | awk -v u="uid=$UID_" '$0 ~ u {for(i=1;i<=NF;i++) if ($i ~ /^(rxBytes|txBytes)=/) b+=$i+0} END {print b+0}' || true; }

HZ=100  # CLK_TCK on Android
start=$(date +%s)
prev_ticks="$(cpu_ticks)"; prev_net="$(net_bytes)"; prev_time="$start"
printf '%-8s %10s %10s %8s %12s\n' "t(s)" "RSS(kB)" "PSS(kB)" "CPU(%)" "net(dB)"
for _ in $(seq 1 "$SAMPLES"); do
  sleep "$INTERVAL"
  now=$(date +%s); ticks="$(cpu_ticks)"; net="$(net_bytes)"
  dt=$((now - prev_time)); dticks=$((ticks - prev_ticks))
  [ "$dt" -gt 0 ] || dt=1
  cpu=$(awk -v d="$dticks" -v t="$dt" -v hz="$HZ" 'BEGIN {printf "%.1f", (d/hz)/t*100}')
  printf '%-8s %10s %10s %8s %12s\n' "$((now - start))" "$(rss_kb)" "$(pss_kb)" "$cpu" "$((net - prev_net))"
  prev_ticks="$ticks"; prev_net="$net"; prev_time="$now"
done

echo
echo "# done — copy this report into STEP0_ANDROID_KOTLIN_REWRITE.md §baseline"
