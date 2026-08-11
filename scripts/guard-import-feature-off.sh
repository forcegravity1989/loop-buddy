#!/usr/bin/env bash
# next 切片七A(design-s7-real-project.md §2.4 第三处硬碰撞/§8.1):导入
# 专用写入面(`bw-store` 的 `ImportStore` trait + 六类历史行类型)只在开了
# `import` 这个 cargo 特性时才编译进 `bw-store`。它收的是完整历史行(含所
# 有时刻与状态),走「撞了就跳过」的插入,不经 `bw_core::can_transition_to`
# 这类合法性守卫——这不是纪律上「有但不许用」,而是结构上「壳的产物里
# 这些方法根本不存在」。这条守卫断言这件事在**真实依赖图**上成立。
#
# 照既有分层守卫(`guard-app-layering.sh`)的做法查 cargo 自己算出来的依
# 赖图,不查 manifest 文本——manifest 文本能被 `[dependencies.bw-store]`
# 表头式、改名依赖等写法绕过,这是仓里已经踩过三次的教训(见
# `guard-app-layering.sh` 头部注释)。查特性同理:`cargo tree -f "{p} [{f}]"`
# 打印的是 cargo 解析后**真正参与这次编译**的特性集,不是 Cargo.toml 里
# 写了什么请求。
#
# 用什么命令查:`cargo tree -p app-desktop -e normal -f "{p} [{f}]"`——
# `-e normal` 只看正式(非 dev/build)依赖边,这正是「壳的产物」的真实依
# 赖图;`bw-app` 的 `[dev-dependencies]` 里如果给 `bw-store` 开了 `import`
# 特性(供 `import_legacy` 指挥器用),那条边不出现在这棵树里,不会被这
# 条守卫误伤——resolver v2 对同一个 crate 的 normal 依赖边与 dev 依赖边
# 分别做特性统一,`app-desktop` 的产物只链接 normal 那一份。
set -euo pipefail
cd "$(dirname "$0")/../next"

echo "… 生成 app-desktop 的真实(非 dev)依赖图特性集(cargo tree -p app-desktop -e normal -f \"{p} [{f}]\")"
tree_output="$(cargo tree -p app-desktop -e normal -f "{p} [{f}]" --prefix none 2>&1)" || {
  echo "✗ 无法生成 app-desktop 的真实依赖图(cargo tree 失败),原文:"
  echo "$tree_output"
  exit 1
}

# `bw-store` 在这棵树里至少出现一次(app-desktop 直接依赖 + 经 bw-app 间
# 接依赖各一条边,cargo tree 按边而不是按包去重打印,行数不固定,但每一
# 行的特性集必须都不含 `import`)。一行都没有反而是更大的问题——说明依
# 赖图本身对不上「app-desktop 依赖 bw-store」这条已知前提,同样判红。
store_lines="$(echo "$tree_output" | grep -E '^bw-store v' || true)"
if [ -z "$store_lines" ]; then
  echo "✗ app-desktop 的真实依赖图里找不到 bw-store 这一行——依赖图生成本身可能有问题,原文:"
  echo "$tree_output"
  exit 1
fi

echo "$store_lines" | sed 's/^/  /'

if echo "$store_lines" | grep -qE '\[[^]]*\bimport\b[^]]*\]'; then
  echo "✗ app-desktop 的真实(非 dev)依赖图里,bw-store 开着 import 特性——"
  echo "  导入专用写入面(ImportStore)会被编译进壳的产物,design-s7-real-project.md"
  echo "  §2.4 第三处硬碰撞的结构约束破了。检查 app-desktop/bw-app 的 [dependencies]"
  echo "  节(不是 [dev-dependencies])有没有误把 features = [\"import\"] 写了进去。"
  exit 1
fi

echo "✓ app-desktop 的真实(非 dev)依赖图里,bw-store 没有开 import 特性(cargo tree -p app-desktop -e normal -f \"{p} [{f}]\" 核验)"
