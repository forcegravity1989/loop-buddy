#!/usr/bin/env bash
# 编排层与内核里不准出现直接进程调用 —— 对外能力只准走连接器接口
# (plan/23 §6:「编排层不准出现连接器字符串分支或直接进程调用」)。
#
# next 切片二A:bw-app 还没建(切片一/四建),这里先只查已存在的目录;
# TARGETS 列出的目录哪个存在就查哪个,不存在就跳过(不当失败处理)——
# 覆盖面会随后续切片把 bw-app 建出来而自动扩大,不需要改这份脚本。
set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS=(
  next/crates/bw-core/src
  next/crates/bw-app/src
)
FORBIDDEN='std::process::Command|tokio::process'

existing=()
for t in "${TARGETS[@]}"; do
  if [ -d "$t" ]; then
    existing+=("$t")
  else
    echo "… $t 尚不存在,跳过(切片进度使然,覆盖面随后续切片扩大)"
  fi
done

if [ "${#existing[@]}" -eq 0 ]; then
  echo "✗ 待查目录一个都不存在,守卫形同虚设——检查 TARGETS 是否配错了路径"
  exit 1
fi

if grep -rnE "$FORBIDDEN" "${existing[@]}"; then
  echo "✗ 编排层/内核出现直接进程调用,对外能力必须经 bw-connector 的四个接口"
  exit 1
fi

echo "✓ 无直接进程调用(已查:${existing[*]})"
