#!/usr/bin/env bash
# scripts/bundle-desktop.sh — 把 debug 二进制包成 dist/BW.app(plan/15 G-1)。
# 启动必须终端直启 Contents/MacOS/ 下的二进制以保留 BW_* env 深链。
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p app-desktop
APP=dist/BW.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/debug/builders-workbench "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>builders-workbench</string>
  <key>CFBundleIdentifier</key><string>com.buildersworkbench.desktop</string>
  <key>CFBundleName</key><string>Builders' Workbench</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "[bundle] ready: BW_DB=<db> $APP/Contents/MacOS/builders-workbench"
