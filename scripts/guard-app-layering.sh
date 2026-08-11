#!/usr/bin/env bash
# next 切片四A:两条查真实依赖图的分层守卫(design-s4-runmanager.md
# §1.2/§2.1/§8):
#
# 1. `bw-app`(编排层)不准依赖 `bw-engine`——PTY/agentcli 那堆原生依赖不该
#    渗进编排层;真跑档需要真连接器时,`bw-engine` 走 `[dev-dependencies]`
#    (指挥器专用,不进正式产物的依赖图),那条路径合法,这条守卫不拦它。
# 2. `bw-store`(存储层)不准依赖 `bw-connector`——存储层因此在编译期就看
#    不见 `ExecState`/`ExecTicket` 这些协议类型,想在存储层写一句「如果执
#    行状态是 X 就把活推到 Y」都写不出来。
#
# next 切片五A 加两条边(design-s5-hexpanel.md §6.2/§10.1 第 2 条):
# 3. `bw-workspace`(本地工作区能力)不准依赖 `bw-connector`——方向单一,
#    连接器反过来依赖它(复用三个 git 辅助),不能反过来。
# 4. `bw-workspace` 不准依赖 `bw-app`(编排层)——它是被编排层调用的一层,
#    不能反过来依赖编排层,否则会形成环。
#
# next 切片五E 再加两条边(design-s5-hexpanel.md §4.5/§10.1 第 2 条「桌面
# 壳是唯一允许出现界面框架依赖的 crate」)——这是**层次意义上**的两条边
# (方向相反,互补,不是同一件事查两遍):
# 5. `bw-app`(编排层,壳正下方那一层,最容易被将来的功能开发不小心带
#    进界面框架依赖的地方)不准依赖 `dioxus`——正向查。这一条与
#    `guard-kernel-ui-free.sh` 的 next 档(逐一查 next 六个内核 crate)有
#    重叠,是刻意的纵深防御:`guard-app-layering.sh` 专管「分层方向对不
#    对」,`guard-kernel-ui-free.sh` 专管「界面框架依赖出现没出现」,两把
#    守卫各自独立失败不互相牵连——哪怕以后有人改坏了其中一把,另一把仍
#    然拦得住这条最高风险的边界。
# 6. `app-desktop`(桌面壳)不准被 `next/` 里任何别的 crate 依赖回
#    去——**反向**查(`cargo tree -i app-desktop`,谁依赖它,而不是它依
#    赖谁)。「壳是唯一允许依赖界面框架的 crate」这句话有两个方向都要
#    守:①没有别的 crate 能把界面框架偷偷带进自己的依赖图(第 5 条 +
#    kernel-ui-free 已经守住);②壳自己必须是这棵依赖图的**终端叶子**,
#    不能反过来被任何人依赖——否则那个反向依赖方也会把 dioxus 间接拖进
#    自己的依赖图,「唯一」这句话就名不副实了。
#
# 评审 Important-3 实测:早期版本用 `awk` 截取 manifest `[dependencies]`
# 小节原文再 grep,能查出朴素的 `crate = { path = … }` 一行式违规,但被
# 两种同样合法的 TOML 写法完全绕过——`[dependencies.bw-engine]` 表头形
# 式、以及改名依赖(`engine = { package = "bw-engine", path = … }`);两
# 种写法 `cargo tree` 确认 bw-engine 真的进了非 dev 依赖图,文本 grep 却
# 认不出来,因为它只认字面的 `bw-engine[[:space:]]*=` 这一种拼写。
#
# 因此改查**解析后的真实依赖图**而不是 manifest 文本:`cargo tree -p
# <crate> -e normal`(只看正式依赖边,`-e normal` 天然排除 dev/build 依
# 赖边,`[dev-dependencies]` 合法路径不受影响)列出的是 cargo 自己算出来
# 的、这个 crate 编译进产物时真正会链接的那棵树——manifest 里用什么句
# 法表达一条依赖,`cargo tree` 都会把它展开成同一种规范形式(真实 crate
# 名),没有第二种绕过写法能骗过它。
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# 用真实依赖图断言 `package` 的**正式**(非 dev)依赖里不含 `forbidden_crate`。
check_absent_from_graph() {
  local workspace_dir="$1"   # next/ 独立 Cargo workspace 所在目录
  local package="$2"         # 被检查的 crate(依赖图的根)
  local forbidden_crate="$3" # 不准出现在正式依赖图里的 crate
  local owner="$4"           # 报错文案里的人话名字

  if [ ! -f "$workspace_dir/crates/$package/Cargo.toml" ]; then
    echo "… $workspace_dir/crates/$package 尚不存在,跳过"
    return
  fi

  local tree_output
  if ! tree_output="$(cd "$workspace_dir" && cargo tree -p "$package" -e normal --prefix none 2>&1)"; then
    echo "✗ 无法生成 $package 的真实依赖图(cargo tree 失败),原文:"
    echo "$tree_output"
    fail=1
    return
  fi

  # `cargo tree --prefix none` 每行形如 `<crate-name> v<version> [...]`
  # ——按真实 crate 名字整词匹配行首,不受 manifest 里怎么拼依赖声明影响
  # (dotted-table / 改名依赖在这里都已经被 cargo 解析成同一个真实包名)。
  if echo "$tree_output" | grep -qE "^${forbidden_crate} v"; then
    echo "✗ $owner 的真实(非 dev)依赖图里出现了 $forbidden_crate —— cargo tree -p $package -e normal 证实"
    echo "  (manifest 写法可能是一行式、[dependencies.$forbidden_crate] 表头、或改名依赖,查真实依赖图三种都拦得住)"
    fail=1
  else
    echo "✓ $owner 的真实(非 dev)依赖图里没有 $forbidden_crate(cargo tree -p $package -e normal 核验)"
  fi
}

# 反向查:断言 `package` **没有任何** `next/` 内的正式(非 dev)反向依赖
# 方——`cargo tree -i` 列出的是"谁依赖它",不是"它依赖谁"。`package` 自身
# 永远会出现在自己的反向依赖图里(它是自己的根),所以期望输出**恰好一
# 行**(它自己);多于一行 = 有别的 crate 反过来依赖了它。
check_no_reverse_deps() {
  local workspace_dir="$1"
  local package="$2"
  local owner="$3"

  if [ ! -f "$workspace_dir/crates/$package/Cargo.toml" ]; then
    echo "… $workspace_dir/crates/$package 尚不存在,跳过"
    return
  fi

  local tree_output
  if ! tree_output="$(cd "$workspace_dir" && cargo tree -i "$package" -e normal --prefix none 2>&1)"; then
    echo "✗ 无法生成 $package 的反向依赖图(cargo tree -i 失败),原文:"
    echo "$tree_output"
    fail=1
    return
  fi

  local line_count
  line_count="$(echo "$tree_output" | grep -c .)"
  if [ "$line_count" -gt 1 ]; then
    echo "✗ $owner 被 $workspace_dir 内别的 crate 正式依赖回去了(cargo tree -i $package -e normal 证实,应当只有它自己一行):"
    echo "$tree_output" | sed 's/^/    /'
    fail=1
  else
    echo "✓ $owner 没有任何反向依赖方(cargo tree -i $package -e normal 只列出它自己)"
  fi
}

check_absent_from_graph "next" "bw-app" "bw-engine" "bw-app(编排层)"
check_absent_from_graph "next" "bw-store" "bw-connector" "bw-store(存储层)"
check_absent_from_graph "next" "bw-workspace" "bw-connector" "bw-workspace(本地工作区能力)"
check_absent_from_graph "next" "bw-workspace" "bw-app" "bw-workspace(本地工作区能力)"
check_absent_from_graph "next" "bw-app" "dioxus" "bw-app(编排层,壳正下方那一层)"
check_no_reverse_deps "next" "app-desktop" "app-desktop(桌面壳)"

if [ "$fail" -ne 0 ]; then
  echo
  echo "编排层不准正式依赖引擎(PTY 等原生依赖不该渗进来);存储层不准正式"
  echo "依赖连接器(看不见协议类型,长不出业务判断)——见"
  echo "design-s4-runmanager.md §1.2 / §2.1。工作区 crate 不准依赖连接器/"
  echo "编排层——方向单一,谁都能依赖它,它谁也不依赖——见"
  echo "design-s5-hexpanel.md §6.2 / §10.1 第 2 条。编排层不准依赖界面框架、"
  echo "桌面壳不准被反向依赖——桌面壳是唯一允许出现界面框架依赖的 crate,"
  echo "这句话的两个方向都要守——见 design-s5-hexpanel.md §4.5 / §10.1 第 2 条。"
  exit 1
fi

echo "✓ 分层守卫全过(编排层/存储层的真实依赖图干净)"
