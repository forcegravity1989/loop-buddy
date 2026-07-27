#!/usr/bin/env bash
# 用法: flow-prep.sh <fixture:demo|fixture:empty|copy:daily> <run-dir> → stdout 打临时 db 路径
set -euo pipefail
SRC_KIND="$1"; RUN_DIR="$2"; mkdir -p "$RUN_DIR/tmp"
DB="$RUN_DIR/tmp/flow-$(date +%s).db"
case "$SRC_KIND" in
  fixture:demo)  cp "$(dirname "$0")/../e2e/fixtures/demo.db" "$DB" ;;
  fixture:empty) : ;;   # 不建文件,app 首启自建 schema
  copy:daily)    cp "$HOME/Library/Application Support/BuildersWorkbench/workbench.db" "$DB" ;;
  *) echo "unknown source: $SRC_KIND" >&2; exit 1 ;;
esac
echo "$DB"
