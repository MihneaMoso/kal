#!/usr/bin/env bash
# Stage the in-repo Android home-screen widgets into the dx-generated Android
# project (which dx creates under target/dx/... during a bundle). The widget
# sources live in android/widget/ (com.kal.calendar.widgets) and are copied
# into the generated app module so they compile into the same APK as the
# native calendar, letting them reuse libmain.so via JNI.
#
#   scripts/stage-widgets.sh          # after `dx bundle` (and apply-icons.sh)
#
# Idempotent: overwrites matching files and patches the manifest so re-runs do
# not duplicate <receiver> entries.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="android/widget/src/main"
APP="$(find target/dx app/target/dx -path '*/app/src/main' -type d 2>/dev/null | head -1 || true)"
if [ -z "$APP" ]; then
  echo "No dx Android project found yet (run a dx bundle first)." >&2
  exit 1
fi

# --- Copy Kotlin sources.
mkdir -p "$APP/kotlin/com/kal/calendar/widgets"
cp -r "$SRC/kotlin/com/kal/calendar/widgets/." "$APP/kotlin/com/kal/calendar/widgets/"

# --- Copy resources. `values/` merges (Android resource merger concatenates
# duplicate <resources> blocks from separate files, but we overwrite whole
# files here, so merge our extra strings/styles into the template's files to
# keep entries like `app_name` and `AppTheme` intact). Other dirs (drawable,
# layout, xml) are plain additions.
merge_values() {
  # $1 = resfile name (strings.xml / styles.xml); writes template file
  #   unchanged if absent, otherwise appends our <resources> entries.
  # Idempotent: skip if a kal_widget entry is already present.
  local target="$APP/res/values/$1"
  if [ -f "$target" ]; then
    python3 - "$target" "$SRC/res/values/$1" <<'PY'
import re, sys
target, src = sys.argv[1], sys.argv[2]
with open(target) as f:
    base = f.read()
with open(src) as f:
    add = f.read()
if "kal_widget" in base or "KalMonthHead" in base:
    sys.exit(0)
head = '<resources'
tail = '</resources>'
def body(s):
    s = s.strip()
    s = re.sub(r'^<\?xml[^>]*>\s*', '', s)
    s = re.sub(r'^<resources[^>]*>\s*', '', s)
    s = re.sub(r'\s*</resources>\s*$', '', s)
    return s.strip()
merged = base.replace(tail, body(add) + '\n' + tail, 1)
merged = re.sub(r'\n{3,}', '\n\n', merged)
with open(target, "w") as f:
    f.write(merged)
PY
  else
    cp "$SRC/res/values/$1" "$target"
  fi
}
mkdir -p "$APP/res/values"
merge_values strings.xml
merge_values styles.xml
for d in drawable layout xml; do
  mkdir -p "$APP/res/$d"
  if [ -d "$SRC/res/$d" ]; then
    cp -rn "$SRC/res/$d/." "$APP/res/$d/"
  fi
done

# --- Patch AndroidManifest.xml: permissions + two widget receivers + boot
# receiver. Use marker comments so the patch is idempotent.
MANIFEST="$APP/AndroidManifest.xml"
python3 - "$MANIFEST" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    xml = f.read()

PERM_MARK = "<!-- kal-widgets:boot-permission -->"
RECV_MARK = "<!-- kal-widgets:receivers -->"

if PERM_MARK not in xml:
    perm = (
        PERM_MARK
        + '\n    <uses-permission android:name="android.permission.RECEIVE_BOOT_COMPLETED" />'
    )
    xml = xml.replace("<!-- Default permissions -->",
                      "<!-- Default permissions -->\n    " + perm, 1)

if RECV_MARK not in xml:
    receivers = (
        "        " + RECV_MARK + "\n"
        '        <receiver android:name="com.kal.calendar.widgets.ScheduleWidgetProvider"\n'
        '            android:exported="true"\n'
        '            android:label="@string/kal_widget_schedule_desc">\n'
        '            <intent-filter>\n'
        '                <action android:name="android.appwidget.action.APPWIDGET_UPDATE" />\n'
        '            </intent-filter>\n'
        '            <meta-data android:name="android.appwidget.provider"\n'
        '                android:resource="@xml/kal_schedule_widget_info" />\n'
        "        </receiver>\n\n"
        '        <receiver android:name="com.kal.calendar.widgets.MonthWidgetProvider"\n'
        '            android:exported="true"\n'
        '            android:label="@string/kal_widget_month_desc">\n'
        '            <intent-filter>\n'
        '                <action android:name="android.appwidget.action.APPWIDGET_UPDATE" />\n'
        '            </intent-filter>\n'
        '            <meta-data android:name="android.appwidget.provider"\n'
        '                android:resource="@xml/kal_month_widget_info" />\n'
        "        </receiver>\n\n"
        '        <receiver android:name="com.kal.calendar.widgets.BootReceiver"\n'
        '            android:exported="true">\n'
        '            <intent-filter>\n'
        '                <action android:name="android.intent.action.BOOT_COMPLETED" />\n'
        '            </intent-filter>\n'
        "        </receiver>\n"
    )
    xml = xml.replace("</application>", receivers + "    </application>", 1)

with open(path, "w") as f:
    f.write(xml)
PY

echo "Widgets staged into $APP"
