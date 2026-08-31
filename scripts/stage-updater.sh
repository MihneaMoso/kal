#!/usr/bin/env bash
# Stage the in-repo Android self-update support (KalUpdater + KalFileProvider)
# into the dx-generated Android project. The APK self-update path needs a
# ContentProvider to serve the staged APK (avoiding FileUriExposedException)
# plus an intent launcher — both live in android/updater/ and are injected into
# the generated app module, mirroring how scripts/stage-widgets.sh works.
#
#   scripts/stage-updater.sh         # after `dx bundle` (and apply-icons.sh)
#
# Idempotent: copies sources and patches the manifest with marker comments so
# re-runs do not duplicate the <provider> entry.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="android/updater/src/main"
APP="$(find target/dx app/target/dx -path '*/app/src/main' -type d 2>/dev/null | head -1 || true)"
if [ -z "$APP" ]; then
  echo "No dx Android project found yet (run a dx bundle first)." >&2
  exit 1
fi

mkdir -p "$APP/kotlin/com/kal/calendar"
cp -n "$SRC/kotlin/com/kal/calendar/KalUpdater.kt" "$APP/kotlin/com/kal/calendar/KalUpdater.kt"
cp -n "$SRC/kotlin/com/kal/calendar/KalFileProvider.kt" "$APP/kotlin/com/kal/calendar/KalFileProvider.kt"

MANIFEST="$APP/AndroidManifest.xml"
python3 - "$MANIFEST" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    xml = f.read()

MARK = "<!-- kal-updater:provider -->"
if MARK not in xml:
    provider = (
        "        " + MARK + "\n"
        '        <provider android:name="com.kal.calendar.KalFileProvider"\n'
        '            android:authorities="com.kal.calendar.updates"\n'
        '            android:exported="false"\n'
        '            android:grantUriPermissions="true" />\n'
    )
    xml = xml.replace("</application>", provider + "    </application>", 1)

with open(path, "w") as f:
    f.write(xml)
PY

echo "Updater staged into $APP"
