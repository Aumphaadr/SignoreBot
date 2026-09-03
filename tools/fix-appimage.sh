#!/usr/bin/env bash
# Пост-обработка AppImage: убираем из AppDir библиотеки GStreamer, которые
# linuxdeploy тянет как зависимости WebKit. Плагины GStreamer грузятся из
# системы и должны видеть системную libgstreamer, иначе <video> в WebKit
# ломается («GStreamer element appsink not found»).
# Использование: tools/fix-appimage.sh  (после npm run tauri build)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="$ROOT/src-tauri/target/release/bundle/appimage"
APPDIR="$BUNDLE/SignoreBot.AppDir"
VER=$(python3 -c "import json;print(json.load(open('$ROOT/src-tauri/tauri.conf.json'))['version'])")
OUT="$BUNDLE/SignoreBot_${VER}_amd64.AppImage"
[ -d "$APPDIR" ] || { echo "нет $APPDIR — сначала npm run tauri build"; exit 1; }
removed=$(find "$APPDIR/usr/lib" -maxdepth 1 -name 'libgst*.so*' | wc -l)
find "$APPDIR/usr/lib" -maxdepth 1 -name 'libgst*.so*' -delete
echo "удалено libgst*: $removed"

# Пустой GST_PLUGIN_SYSTEM_PATH_1_0 от AppRun чинится в самом приложении
# (lib.rs::fix_appimage_gstreamer_env): AppRun.wrapped затирает окружение хуков.
TOOL="$HOME/.cache/tauri/linuxdeploy-plugin-appimage.AppImage"
[ -x "$TOOL" ] || chmod +x "$TOOL"
cd "$BUNDLE"
OUTPUT="$OUT" ARCH=x86_64 "$TOOL" --appdir "$APPDIR" >/tmp/fix-appimage.log 2>&1 || { tail -20 /tmp/fix-appimage.log; exit 1; }
ls -la "$OUT"
