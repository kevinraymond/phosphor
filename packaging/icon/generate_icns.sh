#!/bin/bash
# Generate Phosphor.icns from the master 1024x1024 PNG.
# Requires macOS (uses sips + iconutil).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/icon_1024x1024.png"
ICONSET="$SCRIPT_DIR/Phosphor.iconset"
OUT="$SCRIPT_DIR/Phosphor.icns"

if [ ! -f "$SRC" ]; then
  echo "Error: $SRC not found" >&2
  exit 1
fi

rm -rf "$ICONSET"
mkdir -p "$ICONSET"

for size in 16 32 64 128 256 512 1024; do
  sips -z $size $size "$SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
done

# Create @2x variants
cp "$ICONSET/icon_32x32.png"     "$ICONSET/icon_16x16@2x.png"
cp "$ICONSET/icon_64x64.png"     "$ICONSET/icon_32x32@2x.png"
cp "$ICONSET/icon_256x256.png"   "$ICONSET/icon_128x128@2x.png"
cp "$ICONSET/icon_512x512.png"   "$ICONSET/icon_256x256@2x.png"
cp "$ICONSET/icon_1024x1024.png" "$ICONSET/icon_512x512@2x.png"

# Remove sizes that aren't valid iconset names
rm "$ICONSET/icon_64x64.png" "$ICONSET/icon_1024x1024.png"

iconutil --convert icns "$ICONSET" --output "$OUT"
rm -rf "$ICONSET"

echo "Generated $OUT"
