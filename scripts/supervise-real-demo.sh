#!/bin/zsh
# supervise-run.sh <slug> — 网关抖动期的监理：反复重跑 real_demo 指挥器直到
# 该需求的五阶段环闭合（交接 5 次）。安全性依赖指挥器自身的幂等设计：
# 已成功的阶段绝不重跑；529 失败的调用花费为 $0。
set -u
SLUG="$1"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DB="$ROOT/demo-workspaces/bw-demo.db"
WS="$ROOT/demo-workspaces"
LOG="$ROOT/demo-workspaces/run-$SLUG.log"
cd "$ROOT"
for i in 1 2 3 4 5 6 7 8; do
  {
    echo ""
    echo "=== supervisor attempt $i · $(date '+%F %H:%M:%S') ==="
  } | tee -a "$LOG"
  OUT=$(cargo run -q -p bw-app --example real_demo -- "$DB" "$WS" --only "$SLUG" 2>&1)
  echo "$OUT" >> "$LOG"
  echo "$OUT" | tail -12
  if echo "$OUT" | grep -q "交接 5 次"; then
    echo "=== supervisor: 环已闭合（交接 5 次），成功退出 ===" | tee -a "$LOG"
    exit 0
  fi
  echo "=== supervisor: 尚未闭环，120s 后重试 ===" | tee -a "$LOG"
  sleep 120
done
echo "=== supervisor: 8 次尝试后仍未闭环，如实放弃 ===" | tee -a "$LOG"
exit 1
