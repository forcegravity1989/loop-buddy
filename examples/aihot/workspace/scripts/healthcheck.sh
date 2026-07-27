#!/usr/bin/env bash
# 一键健康检查:真实单测 + 真实冒烟运行。任何一步失败 → 非零码退出。
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== 单测 =="
python3 -m unittest discover tests -v

echo "== 冒烟运行(真实网络,生成今日日报)=="
python3 -m aihot.main

echo "== digests/ 真实产物核对 =="
today="$(date +%Y-%m-%d)"
test -f "digests/${today}.md" || { echo "缺 ${today}.md"; exit 1; }
test -f "digests/${today}.html" || { echo "缺 ${today}.html"; exit 1; }
test -f "digests/index.html" || { echo "缺 index.html"; exit 1; }

echo "== 全部通过 =="
