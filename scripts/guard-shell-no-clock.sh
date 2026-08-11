#!/usr/bin/env bash
# next 切片五E(design-s5-hexpanel.md §4.1/§10.1 第 4 条):新壳的主循环
# 「不持有调度时钟」——plan/23 §9 把「界面层调度时钟」点名成旧工程/v1 都
# 带着的结构病(旧壳每 5 秒问一次有没有定时任务到点、每 100 毫秒抽一次
# PTY 字节)。到点触发/运行轮询这些事归编排层自己(切片四已定:每个运行
# 一个自己的轮询任务),壳里一个定时器构造都不许有。
#
# 这条门禁把这句设计判断变成机器能查的东西——靠人记是记不住的(旧壳与
# v1 都真的犯过这个病)。查的是**壳的源码目录**(TARGETS 列出的目录哪个
# 存在就查哪个,同 guard-no-direct-process.sh 的既有写法,覆盖面随切片
# 推进自动扩大),不是全 workspace——`bw-app::run::manager` 里那些真实的
# 轮询任务(切片四设计稿明文允许的)不在这条门禁的管辖范围内。
set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS=(
  next/crates/app-desktop/src
)
# 定时器构造:`tokio::time::interval`/`interval(`、`tokio::time::sleep`/
# `std::thread::sleep`、`.tick(`(interval 的轮询方法)。**不禁** `Instant::
# now()`/`OffsetDateTime::now_utc()` 这类"读一次当前时刻"的调用——那是给
# 深链 stderr 打时间戳或给推导传时钟参数用的,和"起一个会反复触发的定时
# 器"是两回事,禁了就会把正常的时刻读取也拦下,变成假阳性。
FORBIDDEN='tokio::time::interval|tokio::time::sleep|std::thread::sleep|\.tick\('

existing=()
for t in "${TARGETS[@]}"; do
  if [ -d "$t" ]; then
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
  echo "✗ 壳源码里出现了定时器构造——壳不持有调度时钟(design-s5-hexpanel.md §4.1),到点触发/轮询归编排层自己(每个运行一个自己的轮询任务)"
  exit 1
fi

echo "✓ 壳源码里没有定时器构造(已查:${existing[*]})"
