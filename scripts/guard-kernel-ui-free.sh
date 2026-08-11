#!/usr/bin/env bash
# Non-negotiable #1 (plan 00 §3): the domain kernel must never depend on a UI
# crate. This guard fails CI if any kernel crate pulls dioxus / tauri / wry /
# leptos into its *normal* dependency tree — the wall that keeps desktop & web
# reuse cheap and the wasm32 keepalive honest.
#
# next 切片五E(design-s5-hexpanel.md §10.1 第 1 条):这条守卫**此前完全
# 没覆盖 next/**——下面这份 root 档解析的是仓根 workspace 的同名 crate;
# `next/` 是独立 Cargo workspace(自己的 Cargo.toml/Cargo.lock),它的六个
# 内核 crate 一个都没被这条脚本查过。这不要紧,直到本片把界面框架
# (dioxus)第一次真的带进 `next/`(`app-desktop`)——从这个 commit 起,
# 这条线必须立刻有人守,所以本文件底部加了一份 next 档:同一条判断
# (「除了壳,谁都不准依赖界面框架」),换成对 `next/` 这个独立 workspace
# 跑同一套 `cargo tree` 查询。两档各自独立失败/成功,互不影响。
set -euo pipefail
cd "$(dirname "$0")/.."

FORBIDDEN='dioxus|tauri|wry|leptos|dioxus-desktop'

check_root_workspace() {
  local CARGO="${CARGO:-cargo}"
  local KERNEL=(bw-core bw-engine bw-store bw-app ui)
  local fail=0
  for c in "${KERNEL[@]}"; do
    # Forward dependency tree of the kernel crate, normal edges only.
    if "$CARGO" tree -p "$c" --edges normal --prefix none 2>/dev/null \
        | grep -Eiq "^($FORBIDDEN) "; then
      echo "✗ kernel crate '$c' pulls a UI dependency:"
      "$CARGO" tree -p "$c" --edges normal --prefix none \
        | grep -Ei "^($FORBIDDEN) " | sort -u | sed 's/^/    /'
      fail=1
    else
      echo "✓ $c is UI-free"
    fi
  done
  return "$fail"
}

# next 档(design §10.1 第 1 条):`next/` 六个内核 crate 逐一查——不含
# `app-desktop`,它是"唯一允许出现界面框架依赖的 crate"(design §4.5,
# `scripts/guard-app-layering.sh` 另一条边守这半句的反面:壳不准被别的
# crate 依赖回去)。`app-desktop` 不存在时这个函数整体跳过(切片进度使
# 然),不当失败处理——同 `guard-no-direct-process.sh`/
# `guard-shell-no-clock.sh` 既有的「目录不存在就跳过」写法一致。
check_next_workspace() {
  if [ ! -d "next/crates/app-desktop" ]; then
    echo "… next/crates/app-desktop 尚不存在,next 档跳过(切片进度使然)"
    return 0
  fi
  local KERNEL=(bw-core bw-workspace bw-engine bw-connector bw-store bw-app)
  local fail=0
  for c in "${KERNEL[@]}"; do
    if (cd next && cargo tree -p "$c" --edges normal --prefix none 2>/dev/null) \
        | grep -Eiq "^($FORBIDDEN) "; then
      echo "✗ next 内核 crate '$c' 拉进了界面框架依赖:"
      (cd next && cargo tree -p "$c" --edges normal --prefix none) \
        | grep -Ei "^($FORBIDDEN) " | sort -u | sed 's/^/    /'
      fail=1
    else
      echo "✓ next::$c 无界面依赖"
    fi
  done
  return "$fail"
}

fail=0
check_root_workspace || fail=1
check_next_workspace || fail=1

if [ "$fail" -ne 0 ]; then
  echo
  echo "Kernel crates must stay UI-agnostic (command in, event out). Move the"
  echo "offending dependency into app-desktop / app-web."
  exit 1
fi
echo "All kernel crates (root workspace + next/) are UI-free."
