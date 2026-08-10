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
# next 切片五A(design-s5-hexpanel.md §10.1 第 3 条):`bw-workspace` **不**
# 加进 TARGETS——它的整个存在理由就是起 git 进程(造工作树、git 辅助),
# 和 `bw-engine`(PTY/agentcli 子进程)、`bw-connector`(`gh`/`codehub-cli`
# shell-out)是同一类豁免:这三个 crate 干的活本身就是「对外/对本地环境
# 做真实操作」,门禁只锁住不该起进程的那两层(内核、编排层)。这是一次
# 显式核实过的豁免,不是漏查——如实留痕在这里,别被后来者当成疏漏补上。
# 放宽到 `std::process`(不只 `::Command`)—— `std::process::exit`、
# `use std::process as p` 之类的旁路同样绕过了连接器接口,原来只锁
# `std::process::Command` 会漏掉这些。
FORBIDDEN='std::process|tokio::process'

existing=()
for t in "${TARGETS[@]}"; do
  if [ -d "$t" ]; then
    # 不用 `existing+=("$t")`:macOS 自带的 bash 3.2 在 `set -u` 下,对**零元素**
    # 数组做 `+=`/`"${arr[@]}"` 展开会报 "unbound variable"(3.2 的老 bug,4.4
    # 才修)。这里第一次追加时 existing 还是空数组,会踩中它——改用
    # `${existing[@]+"${existing[@]}"}` 惯用法:数组为空时展开成空,不触发
    # nounset 报错;非空时正常展开。
    existing=("${existing[@]+"${existing[@]}"}" "$t")
  else
    echo "… $t 尚不存在,跳过(切片进度使然,覆盖面随后续切片扩大)"
  fi
done

if [ "${#existing[@]}" -eq 0 ]; then
  echo "✗ 待查目录一个都不存在,守卫形同虚设——检查 TARGETS 是否配错了路径"
  exit 1
fi

if grep -rnE "$FORBIDDEN" "${existing[@]+"${existing[@]}"}"; then
  echo "✗ 编排层/内核出现直接进程调用,对外能力必须经 bw-connector 的四个接口"
  exit 1
fi

echo "✓ 无直接进程调用(已查:${existing[*]})"
