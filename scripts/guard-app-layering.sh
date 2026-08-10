#!/usr/bin/env bash
# next 切片四A:两条只查 manifest **正式依赖**(`[dependencies]`)那一节的
# 分层守卫(design-s4-runmanager.md §1.2/§2.1/§8):
#
# 1. `bw-app`(编排层)不准依赖 `bw-engine`——PTY/agentcli 那堆原生依赖不该
#    渗进编排层;真跑档需要真连接器时,`bw-engine` 走 `[dev-dependencies]`
#    (指挥器专用,不进正式产物的依赖图),那条路径合法,这条守卫不拦它。
# 2. `bw-store`(存储层)不准依赖 `bw-connector`——存储层因此在编译期就看
#    不见 `ExecState`/`ExecTicket` 这些协议类型,想在存储层写一句「如果执
#    行状态是 X 就把活推到 Y」都写不出来。
#
# 只查 `[dependencies]` 小节,不能全文 grep——否则会把合法的 dev 依赖误判
# 成违规。
set -euo pipefail
cd "$(dirname "$0")/.."

# 提取一个 Cargo.toml 的 `[dependencies]` 小节原文:从该行开始,到下一个
# `[` 开头的小节标题之前(没有下一个小节标题就到文件末尾)。
extract_dependencies_section() {
  local manifest="$1"
  awk '
    /^\[dependencies\]/ { printing = 1; next }
    /^\[/ { printing = 0 }
    printing { print }
  ' "$manifest"
}

fail=0

check_absent() {
  local manifest="$1"
  local forbidden_crate="$2"
  local owner="$3"

  if [ ! -f "$manifest" ]; then
    echo "… $manifest 尚不存在,跳过"
    return
  fi

  local section
  section="$(extract_dependencies_section "$manifest")"
  if echo "$section" | grep -qE "^${forbidden_crate}[[:space:]]*="; then
    echo "✗ $owner 的 [dependencies] 里出现了 $forbidden_crate(只准出现在 [dev-dependencies] 里,如果确实需要)"
    fail=1
  else
    echo "✓ $owner 的 [dependencies] 里没有 $forbidden_crate"
  fi
}

check_absent "next/crates/bw-app/Cargo.toml" "bw-engine" "bw-app(编排层)"
check_absent "next/crates/bw-store/Cargo.toml" "bw-connector" "bw-store(存储层)"

if [ "$fail" -ne 0 ]; then
  echo
  echo "编排层不准正式依赖引擎(PTY 等原生依赖不该渗进来);存储层不准正式"
  echo "依赖连接器(看不见协议类型,长不出业务判断)——见"
  echo "design-s4-runmanager.md §1.2 / §2.1。"
  exit 1
fi

echo "✓ 分层守卫全过(编排层/存储层的正式依赖节干净)"
