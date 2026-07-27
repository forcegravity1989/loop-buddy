#!/usr/bin/env bash
# 零依赖语法门禁——本项目没装 ruff/mypy(零 pip 依赖是设计选择),
# 用 stdlib compileall 至少保证全部文件语法合法、可导入。
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -m compileall -q aihot tests
echo "语法门禁通过"
