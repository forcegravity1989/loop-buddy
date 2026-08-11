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
#
# 切片五-3 复审 Important-1 实测:早期版本只匹配单独引入的完整路径
# (`tokio::time::interval`/`tokio::time::sleep` 这两个精确子串),被最常
# 见的分组导入完全绕过——`use tokio::time::{interval, sleep};` 这行文本
# 里出现的是 `tokio::time::{interval`,不是 `tokio::time::interval`,四条
# 旧正则一条都不命中,后续裸调 `interval(..)`/`sleep(..).await` 也没有
# 任何特征可抓。因此这里加两类匹配:①分组导入本身(花括号内出现
# `interval`/`sleep` 字面名,不论是否被 `as` 改名)、②不认限定路径、只
# 认调用点(`interval(`/`interval_at(`/`sleep(`/`sleep_until(`,不论从哪
# 条路径导入、导入时是否改名)——代价是接受少量假阳性(源码里出现同名但
# 与时钟无关的函数会被误伤),换真实覆盖。这与本仓库
# `guard-app-layering.sh` 已经踩过的同一个教训一致(文本匹配清单/导入语
# 法必然有绕过写法)。
#
# 已知局限(文本匹配的天花板,如实注明,不宣称拦得住):
# - 用完全无关的名字重新导出/包一层再调用(如 `mod t { pub use tokio::
#   time::sleep as go; }` 之后在别处 `use crate::t::go; go(d).await;`),
#   跨文件间接到看不出字面 `sleep`/`interval` 的地步,这条门禁看不出来。
# - 用 `Instant::now()`(本守卫明确允许的合法调用)手写忙等循环(如
#   `while Instant::now() < deadline {}`)也是一种"定时器",但结构上和被
#   允许的"读一次当前时刻"完全同形,文本正则区分不了、也不该在这条门禁
#   里硬猜——这类忙等循环留给 `/code-review` 人工判断。
FORBIDDEN='tokio::time::\{[^}]*\b(interval|sleep)\b|tokio::time::interval|tokio::time::sleep|std::thread::sleep|std::thread::\{[^}]*\bsleep\b|\.tick\(|\b(interval|interval_at|sleep|sleep_until)[[:space:]]*\('

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
