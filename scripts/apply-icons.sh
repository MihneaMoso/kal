#!/usr/bin/env bash
# Generate every platform launcher icon from the source art (logo.jpeg at repo
# root) and stage them where the build tooling expects them.
#
#   scripts/apply-icons.sh            # regenerate + copy into dx's Android res/
#
# Generated outputs live under assets/icons/ (committed):
#   desktop/   kal-<size>.png, kal.ico, kal.icns
#   android/   mdpi..xxxhdpi launcher PNGs + adaptive foreground/background
#   ios/       AppIcon.appiconset
#
# In CI / a full local bundle, this script also copies the Android launcher
# PNGs into the dx-generated Android project (target/dx/kal/.../app/src/main/res/),
# because dx hardcodes its template icons and offers no config override.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="logo.jpeg"
OUT="assets/icons"
MAGICK="${MAGICK:-magick}"
command -v "$MAGICK" >/dev/null || { echo "ImageMagick ($MAGICK) not found" >&2; exit 1; }
[ -f "$SRC" ] || { echo "missing $SRC" >&2; exit 1; }

# --- Master: crop the (nearly square) calendar art centered on itself. The
# art's content bbox in logo.jpeg is x475-932 (W457), y123-597 (H474), i.e.
# centered at (703,360). A 600px crop centers it so the calendar sits ~11%
# padded inside a square icon.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
"$MAGICK" "$SRC" -crop 600x600+403+60 +repage -resize 1024x1024 "$TMP/master.png"

# --- Desktop PNGs (Linux .deb / .AppImage use 512 or 256; 128 for menus).
mkdir -p "$OUT/desktop"
for s in 512 256 128 64 48 32; do
  "$MAGICK" "$TMP/master.png" -resize "${s}x${s}" "$OUT/desktop/kal-${s}.png"
done
# Windows .ico (multi-resolution).
"$MAGICK" "$TMP/master.png" \( -clone 0 -resize 16x16 \) \( -clone 0 -resize 32x32 \) \
  \( -clone 0 -resize 48x48 \) \( -clone 0 -resize 256x256 \) \
  -delete 0 "$OUT/desktop/kal.ico"

# --- macOS .icns. ImageMagick can't write real ICNS, so pack the large
# PNG-based chunks (ic07/ic08/ic09/ic10) ourselves with a tiny Python helper.
gen_icns() {
  python3 - "$OUT/desktop/kal.icns" "$TMP/ic07.png" ic07 \
    "$TMP/ic08.png" ic08 "$TMP/ic09.png" ic09 "$TMP/ic10.png" ic10 <<'PY'
import struct, sys, zlib
out = sys.argv[1]
icons = [(sys.argv[i], sys.argv[i+1]) for i in range(2, len(sys.argv), 2)]
def chunk(t, data):
    return struct.pack(">4sI", t.encode(), len(data) + 8) + data
with open(out, "wb") as f:
    body = b""
    for path, tag in icons:
        png = open(path, "rb").read()
        body += chunk(tag, png)
    f.write(struct.pack(">4sI", b"icns", 8 + len(body)))
    f.write(body)
PY
}
"$MAGICK" "$TMP/master.png" -resize 1024x1024 "$TMP/ic10.png"
"$MAGICK" "$TMP/master.png" -resize 512x512  "$TMP/ic09.png"
"$MAGICK" "$TMP/master.png" -resize 256x256  "$TMP/ic08.png"
"$MAGICK" "$TMP/master.png" -resize 128x128  "$TMP/ic07.png"
gen_icns "$OUT/desktop/kal.icns" "$TMP/ic07.png" ic07 \
  "$TMP/ic08.png" ic08 "$TMP/ic09.png" ic09 "$TMP/ic10.png" ic10

# --- Android launcher PNGs (density buckets). dx's template res/ gets
# overwritten below; these committed copies are the durable source.
DENSITY=(mdpi 48 hdpi 72 xhdpi 96 xxhdpi 144 xxxhdpi 192)
mkdir -p "$OUT/android"
for i in $(seq 0 2 $((${#DENSITY[@]}-1))); do
  d="${DENSITY[$i]}"; s="${DENSITY[$((i+1))]}"
  "$MAGICK" "$TMP/master.png" -resize "${s}x${s}" "$OUT/android/ic_launcher_${d}.png"
done

# --- Android adaptive icon (API 26+). Background = brand blue; foreground =
# calendar sized to the 66% safe zone of a 108dp canvas.
AD_TMP="$(mktemp -d)"
"$MAGICK" -size 432x432 "xc:#2D3A4B" "$AD_TMP/bg.png"
# Foreground: art already zoomed with ~11% padding in master; adaptive masks
# crop ~22% of each edge, so keep the calendar ~66% central by scaling to 66%
# and centering on a transparent 432 canvas.
"$MAGICK" "$TMP/master.png" -resize 288x288 \
  \( -size 432x432 xc:none \) +swap -gravity center -composite "$AD_TMP/fg.png"
"$MAGICK" "$AD_TMP/fg.png" -background none -alpha extract "$AD_TMP/fgmask.png"
"$MAGICK" "$AD_TMP/bg.png" "$AD_TMP/fg.png" "$AD_TMP/bg.png" \
  -gravity center -compose over -composite "$OUT/android/ic_launcher.png"
cp "$AD_TMP/fg.png" "$OUT/android/ic_launcher_foreground.png"
cp "$AD_TMP/bg.png" "$OUT/android/ic_launcher_background.png"
rm -rf "$AD_TMP"

# --- iOS AppIcon.appiconset (single-size 1024 master is all modern iOS needs;
# the catalog lists one entry so Xcode scales it).
mkdir -p "$OUT/ios/AppIcon.appiconset"
cp "$TMP/master.png" "$OUT/ios/AppIcon.appiconset/AppIcon.png"
cat > "$OUT/ios/AppIcon.appiconset/Contents.json" <<'JSON'
{
  "images" : [
    {
      "filename" : "AppIcon.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
JSON

echo "Icons generated under $OUT/"

# --- Copy Android launcher icons into the dx-generated Android project (best
# effort; dx creates target/dx/** only during a bundle/serve).
RES="$(find target/dx -path '*/app/src/main/res/drawable*' -type d 2>/dev/null | head -1 || true)"
if [ -n "$RES" ]; then
  for i in $(seq 0 2 $((${#DENSITY[@]}-1))); do
    d="${DENSITY[$i]}"; s="${DENSITY[$((i+1))]}"
    dir="$(echo "$RES" | sed "s/drawable[^/]*/mipmap-${d}/")"
    mkdir -p "$dir"
    cp "$OUT/android/ic_launcher_${d}.png" "$dir/ic_launcher.png"
    # Template ships ic_launcher.webp alongside; drop it so our PNG wins.
    rm -f "$dir/ic_launcher.webp"
  done
  # Adaptive icon lives in mipmap-anydpi-v26; foreground/background drawn as
  # bitmaps copied into drawable/. Replace the template's vector resources.
  DRAW="$(dirname "$RES")/drawable"
  mkdir -p "$DRAW"
  cp "$OUT/android/ic_launcher_foreground.png" "$DRAW/ic_launcher_foreground.png"
  cp "$OUT/android/ic_launcher_background.png" "$DRAW/ic_launcher_background.png"
  rm -f "$DRAW/ic_launcher_foreground.xml" "$DRAW-v24/ic_launcher_foreground.xml" \
        "$DRAW/ic_launcher_background.xml"
  ANYDPI="$(echo "$RES" | sed 's/drawable[^/]*/mipmap-anydpi-v26/')"
  mkdir -p "$ANYDPI"
  cat > "$ANYDPI/ic_launcher.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background"/>
    <foreground android:drawable="@drawable/ic_launcher_foreground"/>
</adaptive-icon>
XML
  cat > "$(echo "$RES" | sed 's/drawable[^/]*/values/')/colors.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="ic_launcher_background">#2D3A4B</color>
</resources>
XML
  echo "Copied launcher icons into dx Android res/ ($RES)"
else
  echo "No dx Android res/ found yet (run a dx bundle first); committed assets/icons/android are the durable source."
fi
