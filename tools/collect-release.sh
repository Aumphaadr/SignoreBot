#!/usr/bin/env bash
# Собирает готовые бандлы в release/ под именами, как в GitHub Releases:
#   SignoreBot_<v>-linux-amd64.deb / .AppImage
#   SignoreBot_<v>-windows-x64-portable.exe / -setup.exe (setup — если собран NSIS)
# Запуск после npm run build:linux и/или build:windows (или tauri build на Windows).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VER=$(python3 -c "import json;print(json.load(open('$ROOT/src-tauri/tauri.conf.json'))['version'])")
T="$ROOT/src-tauri/target"
OUT="$ROOT/release"; mkdir -p "$OUT"
n=0
cp_if() { if [ -f "$1" ]; then cp "$1" "$OUT/$2"; echo "  $2"; n=$((n+1)); fi; }
cp_if "$T/release/bundle/deb/SignoreBot_${VER}_amd64.deb"            "SignoreBot_${VER}-linux-amd64.deb"
cp_if "$T/release/bundle/appimage/SignoreBot_${VER}_amd64.AppImage"  "SignoreBot_${VER}-linux-amd64.AppImage"
cp_if "$T/x86_64-pc-windows-msvc/release/signorebot.exe"             "SignoreBot_${VER}-windows-x64-portable.exe"
cp_if "$T/release/signorebot.exe"                                    "SignoreBot_${VER}-windows-x64-portable.exe"
for f in "$T"/release/bundle/nsis/*-setup.exe "$T"/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe; do
  [ -f "$f" ] && cp_if "$f" "SignoreBot_${VER}-windows-x64-setup.exe"
done
echo "release/: файлов $n (версия $VER)"

# Размеры собранных файлов — в site/release.json (карточки скачивания на сайте);
# после этого `npm run site`. Файлы, которых здесь нет (установщик с Windows),
# сохраняют прежний размер.
python3 - "$ROOT" "$VER" <<'PY'
import json, os, sys
root, ver = sys.argv[1], sys.argv[2]
p = os.path.join(root, "site", "release.json")
d = json.load(open(p))
names = {"setup": f"SignoreBot_{ver}-windows-x64-setup.exe", "portable": f"SignoreBot_{ver}-windows-x64-portable.exe",
         "zip": f"SignoreBot_{ver}-windows-x64-portable.zip", "deb": f"SignoreBot_{ver}-linux-amd64.deb", "appimage": f"SignoreBot_{ver}-linux-amd64.AppImage"}
changed = []
for kind, name in names.items():
    f = os.path.join(root, "release", name)
    if os.path.isfile(f) and d["sizes"].get(kind) != os.path.getsize(f):
        d["sizes"][kind] = os.path.getsize(f); changed.append(kind)
json.dump(d, open(p, "w"), ensure_ascii=False, indent=2); open(p, "a").write("\n")
print("site/release.json: размеры обновлены — " + (", ".join(changed) if changed else "без изменений") + "; пересоберите сайт: npm run site")
PY
