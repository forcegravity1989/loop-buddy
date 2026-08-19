#!/usr/bin/env bash
# scripts/bundle-desktop.sh — 把 debug 二进制包成一个 macOS .app。
#
#   ./scripts/bundle-desktop.sh        # 老壳 app-desktop → dist/BW.app
#   ./scripts/bundle-desktop.sh v4     # 新壳 app-shell   → dist/BW-V4.app
#
# 启动必须**终端直启** Contents/MacOS/ 下的二进制,`open -a` 传不进 BW_* 环境
# 变量,深链就没了。
#
# Windows 安装包不在这个脚本里,也不在这个仓里 —— 见 docs/LEFTOVERS.md。
set -euo pipefail
cd "$(dirname "$0")/.."

SHELL_KIND="${1:-v3}"
case "$SHELL_KIND" in
  v3) CRATE=app-desktop; BIN=builders-workbench; APP=dist/BW.app
      NAME="Builders' Workbench"; ID=com.buildersworkbench.desktop; VER=0.3.0-v3 ;;
  v4) CRATE=app-shell;   BIN=bw-v4-dev;          APP=dist/BW-V4.app
      NAME="Builders' Workbench V4"; ID=com.buildersworkbench.v4; VER=0.4.0-v4 ;;
  *)  echo "用法:$0 [v3|v4]" >&2; exit 2 ;;
esac

cargo build -p "$CRATE"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "target/debug/$BIN" "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIdentifier</key><string>$ID</string>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VER</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "[bundle] ready: BW_DB=<db> $APP/Contents/MacOS/$BIN"
