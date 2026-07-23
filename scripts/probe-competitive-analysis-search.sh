#!/usr/bin/env bash
# probe-competitive-analysis-search.sh — C10 票(plan/13 D9)的检索探活。
#
# 竞品分析 Skill(docs/skills/competitive-analysis/SKILL.md)要求「执行器
# 联网检索能力先探活,不通则如实降级为『人喂材料+agent 整理』」。本脚本用
# 真实 `claude -p` CLI 跑一个极小的联网检索任务(查一个公开事实并要求给出
# 来源 URL),预算用既有 `--max-budget-usd` 机制封顶,超时兜底,最多退避重
# 试 2 次——不是常绿 CI 步骤,是监理脚本形态的一次性/偶发性真实探针,和
# scripts/supervise-real-demo.sh 同一纪律:真实 claude 执行只在这类脚本
# 里跑,幂等可重试,不进日常门禁。
#
# 任何结果都是合法结果——通/不通/账号配额/网关抖动都要如实记录,不是只有
# "成功"才算完成本脚本的使命。见 CLAUDE.md「mock 必须自我标注」「E2E 验
# 证绝不依赖网关」。
#
# 用法:
#   ./scripts/probe-competitive-analysis-search.sh
#   BW_CLAUDE_MAX_BUDGET_USD=0.30 ./scripts/probe-competitive-analysis-search.sh
#
# 输出:stdout 打印分类结论;完整原始 JSON 落盘到
#   demo-workspaces/.probe-competitive-analysis-search.json
# 供 docs/skills/competitive-analysis/PROBE.md 人工誊写(本脚本不自动改写
# PROBE.md——那份文件记的是"某次真实探测的历史记录",不是滚动覆盖的日志)。

set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/demo-workspaces"
mkdir -p "$OUT_DIR"
RAW_OUT="$OUT_DIR/.probe-competitive-analysis-search.json"
STDERR_OUT="$OUT_DIR/.probe-competitive-analysis-search.stderr.log"

CLAUDE_BIN="${BW_CLAUDE_BIN:-claude}"
# 最低可用值,真实探测定的(不是拍脑袋):2026-07-23 首跑 0.15 美元直接
# error_max_budget_usd 腰斩——原始 JSON 显示 haiku 子模型确实发起了一次
# web_search_requests(检索能力本身可用),但主模型汇总+引用来源那一轮还
# 没跑完预算就没了。加到 0.30 美元后完整跑通(真实花费约 $0.15,曲线因
# 缓存命中而波动,0.30 留出安全边际)。低于 real_demo.rs 整段 playbook
# 调用的 0.75,因为探针只是一次单轮检索问答,不做文件编辑。详见
# docs/skills/competitive-analysis/PROBE.md 的完整记录。
MAX_BUDGET_USD="${BW_CLAUDE_MAX_BUDGET_USD:-0.30}"
TIMEOUT_SECS=180
# 与 bw-engine/src/claude_cli.rs 的瞬时网关退避同口径,但探针只是一次性
# 验证,不是产品执行路径——固定 2 次上限,绝不无限重试。
BACKOFF_SECS=(20 60)

PROMPT='请检索一个公开、可核实的事实——例如:Rust 编程语言当前最新稳定版本号是什么——并在回答里给出你依据的信息来源 URL。如果你没有可用的联网检索工具,直接明确说明"我没有可用的检索工具",不要凭训练记忆编造来源或版本号。'

echo "[probe] claude 二进制: $(command -v "$CLAUDE_BIN" 2>/dev/null || echo "$CLAUDE_BIN (未在 PATH 解析到)")"
echo "[probe] max-budget-usd: $MAX_BUDGET_USD"
echo "[probe] 超时: ${TIMEOUT_SECS}s · 最多重试 ${#BACKOFF_SECS[@]} 次(仅瞬时网关错误触发)"

is_transient_gateway_error() {
  # 与 bw-engine/src/claude_cli.rs::is_transient_gateway_error 同口径。
  case "$1" in
    *"API Error: 529"*|*"API Error: 503"*|*"API Error: 502"*|*"API Error: 504"*|*"访问量过大"*) return 0 ;;
  esac
  echo "$1" | grep -qi "overloaded" && return 0
  return 1
}

# macOS 默认无 coreutils `timeout`/`gtimeout`——用后台进程+定时 kill 自
# 己实现一个够用的超时兜底,不额外要求用户装 coreutils。
run_with_timeout() {
  # $1 = 超时秒数,$2 = 输出落盘文件,其余是要跑的命令。
  local secs="$1" outfile="$2"
  shift 2
  "$@" >"$outfile" 2>>"$STDERR_OUT" &
  local cmd_pid=$!
  (
    sleep "$secs"
    kill -TERM "$cmd_pid" 2>/dev/null
  ) &
  local watcher_pid=$!
  wait "$cmd_pid" 2>/dev/null
  local status=$?
  kill "$watcher_pid" 2>/dev/null
  wait "$watcher_pid" 2>/dev/null
  return $status
}

attempt=0
result_json=""
last_err=""
while :; do
  attempt=$((attempt + 1))
  echo ""
  echo "[probe] 第 $attempt 次尝试 …"
  # 与 ClaudeCliExecutor 同一条纪律:嵌套子 claude 剥离宿主会话级凭据/网关
  # 环境变量,让子进程回落到用户自己的 CLI 配置——探针要测的是"用户真实
  # 配置下检索能力通不通",不是宿主会话的临时令牌。
  attempt_out="$OUT_DIR/.probe-attempt-$attempt.json"
  : > "$STDERR_OUT"
  run_with_timeout "$TIMEOUT_SECS" "$attempt_out" \
    env -u ANTHROPIC_AUTH_TOKEN -u ANTHROPIC_BASE_URL -u ANTHROPIC_MODEL \
        -u CLAUDECODE -u CLAUDE_CODE_SESSION_ID -u CLAUDE_CODE_CHILD_SESSION \
        -u CLAUDE_CODE_ENTRYPOINT \
        "$CLAUDE_BIN" -p "$PROMPT" \
          --output-format json \
          --no-session-persistence \
          --max-budget-usd "$MAX_BUDGET_USD" \
          --permission-mode acceptEdits
  status=$?
  out="$(cat "$attempt_out" 2>/dev/null || true)"
  rm -f "$attempt_out"

  if [ $status -eq 143 ] && [ -z "$out" ]; then
    last_err="超时(>${TIMEOUT_SECS}s,子进程已被本脚本的 timeout 兜底杀掉)"
    echo "[probe] $last_err"
  elif [ $status -ne 0 ] && [ -z "$out" ]; then
    last_err="claude CLI 非零退出($status),stdout 为空;stderr: $(tr '\n' ' ' < "$STDERR_OUT" | cut -c1-300)"
    echo "[probe] $last_err"
  else
    result_json="$out"
    echo "$out" > "$RAW_OUT"
    echo "[probe] 拿到 stdout,已落盘 $RAW_OUT"
    break
  fi

  if [ $attempt -le ${#BACKOFF_SECS[@]} ] && is_transient_gateway_error "$last_err"; then
    delay=${BACKOFF_SECS[$((attempt - 1))]}
    echo "[probe] 判定为瞬时网关错误,${delay}s 后重试(第 $attempt/${#BACKOFF_SECS[@]} 次退避)…"
    sleep "$delay"
    continue
  fi
  break
done

echo ""
echo "════════════════ 结论 ════════════════"
if [ -z "$result_json" ]; then
  echo "结论:不通 · 原因:$last_err"
  if echo "$last_err" | grep -qi "529\|overloaded\|访问量过大"; then
    echo "分类:网关抖动(529 类)"
  elif echo "$last_err" | grep -qi "429\|quota\|rate.limit\|too many requests"; then
    echo "分类:账号配额(429 类)"
  else
    echo "分类:其它(见上方原始错误文本,人工判读)"
  fi
  exit 1
fi

is_error=$(echo "$result_json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("is_error", False))' 2>/dev/null || echo "unknown")
if [ "$is_error" = "True" ] || [ "$is_error" = "true" ]; then
  echo "结论:不通(CLI 返回 is_error=true)"
  echo "$result_json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("subtype:", d.get("subtype","")); print("errors:", d.get("errors", [])); print("result:", d.get("result",""))' 2>/dev/null || echo "$result_json"
  exit 1
fi

result_text=$(echo "$result_json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("result",""))' 2>/dev/null || echo "$result_json")
echo "结论:CLI 调用成功(is_error=false)。原始文本摘要(前 500 字):"
echo "$result_text" | cut -c1-500
echo ""
if echo "$result_text" | grep -qiE "https?://"; then
  echo "分类:检索可用——回答里含 URL(需人工核对该 URL 是否真的是检索得到,而非编造)"
else
  echo "分类:回答未含 URL——可能是「无检索工具」的诚实降级声明,也可能是拒绝或答非所问,人工核对上方文本"
fi
