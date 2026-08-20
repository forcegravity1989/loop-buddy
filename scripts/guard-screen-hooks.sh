#!/usr/bin/env bash
# 用了 Dioxus hook 的函数必须是组件(`#[component]`)。
#
# 为什么要这条(2026-08-19 评审抓到的真 bug,编译期一点征兆都没有):
# 普通函数在 rsx! 里被直接调用时,它的 use_signal 落在**调用方**的 hook 表上,
# 按序号取。切一次屏,序号还在、类型换了(接入屏那格是 String,计划屏那格是
# Option<CardItemVm>),Dioxus 取 hook 时 downcast 失败,整次渲染 panic ——
# 用户看到的就是一块空白面板。放在 for 循环里更糟:hook 数量随列表长度变,
# 行与行的输入框内容会串位。
#
# 做成组件,每个组件有自己的作用域,序号各算各的。
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=crates/app-shell/src

if [ ! -d "$ROOT" ]; then
  echo "跳过:$ROOT 还不存在"
  exit 0
fi

fail=0
while IFS= read -r -d '' f; do
  bad=$(awk '
    /^[[:space:]]*#\[component\]/ { is_component = 1; next }
    # 注释行整行跳过 —— 文档注释里提一句 use_context 不是在调 hook。
    /^[[:space:]]*\/\// { next }
    /^[[:space:]]*(pub )?(async )?fn / {
      fn_line = NR; fn_name = $0; fn_is_component = is_component; is_component = 0; next
    }
    /use_signal\(|use_future\(|use_hook\(|use_effect\(|use_memo\(|use_resource\(|use_context/ {
      if (fn_is_component != 1) printf "    第 %d 行的 hook 在非组件函数里(函数定义在第 %d 行)\n", NR, fn_line
    }
    # 属性之外的任何非空行都会清掉 #[component] 标记,避免它跨过无关代码生效
    !/^[[:space:]]*(#\[|\/\/)/ && NF { is_component = is_component }
  ' "$f")
  if [ -n "$bad" ]; then
    echo "✗ $f"
    echo "$bad"
    fail=1
  fi
done < <(find "$ROOT" -name '*.rs' -print0)

if [ "$fail" -ne 0 ]; then
  echo
  echo "给这些函数加 #[component],并在调用点用 rsx! { Xxx { .. } } 渲染。"
  exit 1
fi
echo "app-shell 里所有用 hook 的函数都是组件。"
