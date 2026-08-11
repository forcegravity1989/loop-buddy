#!/usr/bin/env bash
# next 切片七A/七-1 修复轮(design-s7-real-project.md §2.4 第三处硬碰撞/
# §8.1,task-s7a-review.md Critical-1):导入专用写入面(`ImportStore` trait
# + 六类历史行类型 + `schema_import.sql`)住在独立 crate `bw-store-import`
# 里(七-1 修复轮之前是 `bw-store` 的一个 `import` cargo 特性——2026-08-11
# 复审用编译期探针 + 产物比对 + 运行时建表三重实证发现:cargo 的特性统一
# 对同一个 crate、同一个目标平台、同一次调用里的全部请求方一视同仁,不分
# normal/dev 依赖边;仓库门禁自己跑的 `cargo check/clippy --workspace
# --all-targets` 恰好会把 `bw-app` [dev-dependencies] 那份带 import 特性的
# `bw-store` 与 `app-desktop` 依赖的那份统一编译成同一份产物,原来那句
# 「resolver v2 分别做特性统一」的注释是错的)。结构解法:导入面挪进一个
# `app-desktop` 永远不会依赖的独立 crate——没有特性可统一,壳的编译图上
# 连这个 crate 的名字都不存在,不管用哪种 cargo 调用方式都是如此。
#
# 这条守卫因此分两档:
#   [隔离构建档] 原有检法(查 cargo tree 算出来的依赖图),照旧留着——对
#     `cargo build -p app-desktop` 这种单独构建成立,但**不是**门禁真正跑
#     的构建形态,如实改名,不再暗示它覆盖 workspace 统一构建。
#   [workspace 统一构建产物档] 新增,直接复现门禁那条 `--workspace
#     --all-targets` 调用、检查 app-desktop 链出来的**产物二进制**本身有
#     没有导入专用 schema 的字节痕迹——查产物,不查 cargo 的解析结果
#     (「查代理不查真实」的老坑,C1 复审踩过一次,同款教训)。
#
# 档位参数(七-1 修复轮 CI 接线,主控裁决,2026-08-11,
# task-s7a-review.md Important-5):[workspace 统一构建产物档] 要
# `cargo build --workspace --all-targets`,会真的编译并链接
# `app-desktop`(dioxus)——CI 的 next-gates job 刻意不编桌面壳(ubuntu-
# latest runner 没装 libwebkit2gtk/libgtk-3 等系统开发包,见 ci.yml
# `next-gates` job 排除 app-desktop 那段注释,同一条理由),这一档因此接
# 不进 CI。[隔离构建档] 只跑 `cargo tree`,不编译任何东西,能接。两档分
# 工不变(隔离档答"声明的依赖图对不对"、产物档答"这次真实构建产物里有
# 没有泄漏",互不替代),变的只是"哪一档进 CI 常绿、哪一档留本地/终审":
#   不给参数(默认)——两档都跑,与七-1 修复轮之前的行为完全一样,本地
#     门禁与终审用这个形态。
#   `--dep-edge-only` ——只跑 [隔离构建档],跳过需要编壳的
#     [workspace 统一构建产物档];CI 的 next-gates job 用这个形态(见
#     ci.yml 对应步骤旁的注释)。
set -euo pipefail
cd "$(dirname "$0")/../next"

mode="${1:-all}"
case "$mode" in
  all|"") ;;
  --dep-edge-only) ;;
  *)
    echo "✗ 未知参数:$mode(合法值:不给参数——两档全跑;或 --dep-edge-only——只跑隔离构建档)" >&2
    exit 1
    ;;
esac

# ── [隔离构建档] app-desktop 自己声明的依赖图 ──────────────────────────
# 查 cargo 自己算出来的依赖图,不查 manifest 文本——manifest 文本能被
# `[dependencies.bw-store]` 表头式、改名依赖等写法绕过,这是仓里已经踩过
# 三次的教训(见 `guard-app-layering.sh` 头部注释)。这一档只回答「app-
# desktop 自己有没有在 Cargo.toml 里显式请求 bw-store-import 或
# bw-store 的 import 特性」——回答不了「这次真实构建里 cargo 有没有替它
# 把别的包request 的东西统一进来」,那是下面第二档的事。
echo "… [隔离构建档] app-desktop 的真实(非 dev)依赖图(cargo tree -p app-desktop -e normal)"
tree_output="$(cargo tree -p app-desktop -e normal -f "{p} [{f}]" --prefix none 2>&1)" || {
  echo "✗ 无法生成 app-desktop 的真实依赖图(cargo tree 失败),原文:"
  echo "$tree_output"
  exit 1
}

if echo "$tree_output" | grep -qE '^bw-store-import v'; then
  echo "✗ [隔离构建档] app-desktop 的真实(非 dev)依赖图里出现了 bw-store-import——"
  echo "  导入专用写入面不该有任何路径能被壳依赖到,检查 app-desktop/bw-app 的"
  echo "  [dependencies] 节(不是 [dev-dependencies])有没有误加了这一行。"
  exit 1
fi

store_lines="$(echo "$tree_output" | grep -E '^bw-store v' || true)"
if [ -z "$store_lines" ]; then
  echo "✗ app-desktop 的真实依赖图里找不到 bw-store 这一行——依赖图生成本身可能有问题,原文:"
  echo "$tree_output"
  exit 1
fi
echo "$store_lines" | sed 's/^/  /'
if echo "$store_lines" | grep -qE '\[[^]]*\bimport\b[^]]*\]'; then
  echo "✗ [隔离构建档] app-desktop 的真实(非 dev)依赖图里,bw-store 还留着 import 特性——"
  echo "  七-1 修复轮已经把这个特性从 bw-store 里删掉了,出现说明有代码没跟着改。"
  exit 1
fi
echo "✓ [隔离构建档] 没有 bw-store-import 依赖边,bw-store 也没有 import 特性(cargo tree -p app-desktop -e normal 核验)"

if [ "$mode" = "--dep-edge-only" ]; then
  echo "… [workspace 统一构建产物档] 跳过(--dep-edge-only:CI 形态,不编桌面壳——本地/终审用不带参数的全档形态覆盖这一档)"
else
  # ── [workspace 统一构建产物档] 复现门禁真实构建形态,检产物 ────────────
  # 门禁那条 `cargo check/clippy --workspace --all-targets` 会把整个
  # workspace(含 bw-app 的 [dev-dependencies])放进同一次特性/依赖解析。这
  # 一档直接跑 `cargo build --workspace --all-targets`(复用默认 target 目
  # 录——门禁前面几步的 `cargo check/clippy --workspace --all-targets` 已经
  # 把大部分类型检查/借用检查的中间产物缓存在这里,这一步只是在同一份缓存
  # 上补完代码生成 + 链接,不是从头再编一遍),然后直接 `strings` 链出来的
  # app-desktop 二进制,断言导入专用 schema(`schema_import.sql` 里的建表语
  # 句)不在里面——这是对**产物**的检查,不是对 cargo 解析结果的检查,
  # `bw-store-import` 即使被误统一进某个中间产物,只要它没有被 app-desktop
  # 的任何一条真实调用路径引用到,链接器的死代码剥离也会把它剥掉(见
  # task-s7a-review.md Evidence 2「import_impl 的 45 个符号被剥掉」的既有先
  # 例);唯一会让这条断言翻红的,是 app-desktop 的产物代码路径上**真的**存
  # 在一条会执行到导入 schema 的调用链。
  #
  # 这一档因此不接进 CI(next-gates job 刻意不编桌面壳,理由见本文件头部
  # 「档位参数」一节 + ci.yml `next-gates` job 排除 app-desktop 那段注
  # 释)——留本地开发机与终审跑,不进常绿门禁。
  echo "… [workspace 统一构建产物档] cargo build --workspace --all-targets(复现门禁真实构建形态)"
  build_log="$(mktemp)"
  if ! cargo build --workspace --all-targets >"$build_log" 2>&1; then
    echo "✗ workspace 统一构建失败,原文见下(尾 40 行):"
    tail -n 40 "$build_log"
    rm -f "$build_log"
    exit 1
  fi
  rm -f "$build_log"

  bin_path="target/debug/bw-next"
  if [ ! -f "$bin_path" ]; then
    echo "✗ 找不到 app-desktop 的产物二进制($bin_path)——workspace 统一构建没有产出它,这一档检法本身失效"
    exit 1
  fi

  leak_needles=("CREATE TABLE IF NOT EXISTS import_ledger" "CREATE TABLE IF NOT EXISTS artifact_archive")
  leaked=0
  for needle in "${leak_needles[@]}"; do
    hits="$(strings "$bin_path" | grep -c -F "$needle" || true)"
    if [ "$hits" -gt 0 ]; then
      echo "✗ [workspace 统一构建产物档] app-desktop 产物二进制($bin_path)里出现了「$needle」——"
      echo "  导入专用 schema 在门禁真跑的 --workspace --all-targets 构建形态下泄漏进了壳的产物。"
      leaked=1
    fi
  done
  if [ "$leaked" -ne 0 ]; then
    exit 1
  fi
  echo "✓ [workspace 统一构建产物档] app-desktop 产物二进制($bin_path)里没有导入专用 schema 的痕迹(strings 核验,门禁构建形态下复测)"
fi
