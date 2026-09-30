#!/bin/sh
# Regenerates every platform's app icon from icon/icon.svg, the one drawing there is.
#
# The outputs are checked in, so building any of the apps never needs this script —
# only changing the drawing does. Needs rsvg-convert and ImageMagick's convert, and
# iconutil, so it runs on a Mac.
#
# The drawing is a 1024 canvas holding an 824 rounded tile, which is already the
# macOS icon grid. The other platforms want the picture without that tile: iOS and
# Android cut their own shape out of a full square, so for them the tile's clip is
# lifted and the painted background is allowed to run past its corners.
set -eu

cd "$(dirname "$0")/.."
src=icon/icon.svg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# render SVG SIZE OUT [VIEWBOX]: rasterise, optionally through a different viewBox.
render() {
    svg=$1
    if [ -n "${4:-}" ]; then
        svg="$tmp/view.svg"
        sed "s/viewBox=\"[^\"]*\"/viewBox=\"$4\"/" "$1" > "$svg"
    fi
    rsvg-convert -w "$2" -h "$2" "$svg" -o "$3"
}

# The tile's clip widened to the whole plane, and the flat blue grown to match,
# so any square cut from it is painted edge to edge.
bleed="$tmp/bleed.svg"
sed -e 's|<rect x="100" y="100" width="824" height="824" rx="185"/>|<rect x="-400" y="-400" width="1824" height="1824"/>|' \
    -e 's|<rect x="100" y="100" width="824" height="824" fill=|<rect x="-400" y="-400" width="1824" height="1824" fill=|' \
    "$src" > "$bleed"
background=$(sed -n 's/.*<rect x="100" y="100" width="824" height="824" fill="\(#[0-9A-Fa-f]*\)".*/\1/p' "$src")

# macOS: the drawing as it is.
set_dir="$tmp/AppIcon.iconset"
mkdir "$set_dir"
for size in 16 32 128 256 512; do
    render "$src" "$size" "$set_dir/icon_${size}x${size}.png"
    render "$src" $((size * 2)) "$set_dir/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$set_dir" -o macos/AppIcon.icns

# Linux: GNOME takes the SVG itself.
cp "$src" linux/dev.artwindow.svg

# Windows: the tile alone, since a 16-pixel icon cannot spare a fifth of itself to
# margin. PNG-compressed entries, which every Windows since Vista reads.
for size in 16 20 24 32 40 48 64 256; do
    render "$src" "$size" "$tmp/win-$size.png" "90 90 844 844"
done
python3 - "$tmp" windows/art-window.ico <<'PY'
import struct, sys
tmp, out = sys.argv[1], sys.argv[2]
sizes = [16, 20, 24, 32, 40, 48, 64, 256]
images = [open(f"{tmp}/win-{s}.png", "rb").read() for s in sizes]
header = struct.pack("<HHH", 0, 1, len(images))
offset = 6 + 16 * len(images)
entries = b""
for size, data in zip(sizes, images):
    side = 0 if size == 256 else size
    entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
open(out, "wb").write(header + entries + b"".join(images))
PY

# iOS: one opaque 1024 square of the unclipped picture; the system rounds it.
render "$bleed" 1024 "$tmp/ios.png" "100 100 824 824"
convert "$tmp/ios.png" -background "$background" -alpha remove -alpha off \
    ios/ArtWindow/Assets.xcassets/AppIcon.appiconset/AppIcon.png

# Android: an adaptive icon's foreground is 108dp of which a launcher may show only
# the central 66dp circle, so the window has to shrink into that; the viewBox is
# widened until it does. The flat blue is the background layer, as a colour.
res=android/app/src/main/res
for pair in mdpi:108 hdpi:162 xhdpi:216 xxhdpi:324 xxxhdpi:432; do
    density=${pair%%:*}
    mkdir -p "$res/mipmap-$density"
    render "$bleed" "${pair##*:}" "$res/mipmap-$density/ic_launcher_foreground.png" "-72 -72 1168 1168"
done
sed -i '' "s|<color name=\"ic_launcher_background\">[^<]*<|<color name=\"ic_launcher_background\">$background<|" \
    "$res/values/colors.xml"

# Android TV: the 320x180dp banner, the picture centred on its own blue.
mkdir -p "$res/drawable-xhdpi"
render "$bleed" 360 "$tmp/banner-square.png" "0 0 1024 1024"
convert -size 640x360 "xc:$background" "$tmp/banner-square.png" -gravity center -composite \
    "$res/drawable-xhdpi/tv_banner.png"

echo "icons regenerated from $src"
