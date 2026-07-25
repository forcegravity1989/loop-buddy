#!/usr/bin/env python3
"""gen-flow-report.py — 把一次验收跑的机器产物装配成一页给人看的 HTML 证据报告。

用法:
    python3 scripts/gen-flow-report.py <run-dir>

写出 `<run-dir>/report.html`,并把它的路径打印到 stdout。

输入目录结构(由验收跑产生,本脚本只读,不改):
    <run-dir>/
      meta.json                     {"batch": "...", "started_at": "...", "commit": "..."}
      flows/
        <flow-name>/                # flow-name = e2e/flows/core/<flow-name>.toml 去掉后缀
          cmds                      # 追加过去的命令文件(人可读,报告不展示——见下)
          cmds.log                  # 驱动自产的结果日志:唯一的主证据
          readback.jsonl            # 每行 {"sql": "...", "raw": "<sqlite3 原始输出>"}
          snaps/<名字>.png          # 截图(可能缺失)

为什么这里没有用 tomllib 做真正的 TOML 解析
--------------------------------------------
这台开发机的系统 python3 是 3.9.6(Apple 自带的 /usr/bin/python3),`tomllib`
直到 3.11 才进标准库,这里没有。和协调者确认过,三条常见的"绕过去"的路都不
走:
  1. 不装 python@3.11 —— 会让这个工具依赖一台机器上不一定有的非默认解释
     器,对一个把"可复现证据"当命根子的仓库来说是长期脆弱性,不值。
  2. 不装 pip 第三方包(如 `tomli`)—— 违反本仓库 python 脚本 stdlib-only
     的纪律(`scripts/make_demo_video.py` 除外那是历史遗留,且本任务 brief
     明确要求 stdlib only)。
  3. 不手写一个通用 TOML 解析器 —— 过度工程;而且解析细节一旦有偏差,报告
     可能悄悄读错 expect/sql 却不自知,这和本仓库"绝不假绿"的纪律直接冲突。

所以下面 `scan_flow_toml()` 做的是**逐行字面量提取**,不是解析:只认「我们
自己手写、格式固定」的 `e2e/flows/core/*.toml` 考卷文件里几种已知形状的
行,别的一律不碰。前提假设(任何一条不成立,提取就"如实报错",绝不悄悄
猜一个近似值):

  - `key = "value"` 单行赋值,值两端是 ASCII 双引号,值内部不含 ASCII 双
    引号(考卷里用「」和中文单引号 ' ',从不在字符串值内嵌 ASCII `"`)。
  - `[[verify]]` / `[[step]]` 是各自数组表的唯一开启标记,新块在下一个
    `[[...]]` 行或文件结尾处关闭——不支持嵌套。
  - 只关心这几个 key:顶层的 `name`/`purpose`(各自只取第一次出现);
    `[[verify]]` 块内的 `sql`/`expect`;`[[step]]` 块内可选的 `snap`(不是
    coordinator delta 里点名要的字段,但"缺图:<名>" 这条不变的验收要求
    必须知道"考卷期望拍哪些截图",而截图名就只写在这里——同一套逐行字面
    量提取顺手覆盖,不是另起一套机制)。
  - 任何一行长得像 `key = ...` 但值没有干净地以 `"..."` 收尾 → 判定为
    「提取失败」,把原始行文本原样记下来,绝不猜一个近似值糊弄过去。

考卷来源解析顺序(coordinator 修订):优先 `<run-dir>/flows/<flow>/flow.toml`
(验收跑把考卷拷贝在证据旁边,archive 下来的报告自包含);找不到才退回仓库
里的 `e2e/flows/core/<flow>.toml`;两处都没有 → 照常展示日志,标注「考卷
缺失,无法对照 purpose/expect」,绝不因此崩溃。
"""

import base64
import html
import json
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# 设计 token(plan/00 §6)
# ---------------------------------------------------------------------------

PAPER = "#EFEBE2"
CLAY = "#C5654A"
INK = "#2b2622"
GREEN = "#4a7c59"
RED = "#b0413e"
GRAY = "#8a8578"

STATUS_LABEL = {
    "green": "绿 · 全过",
    "red": "红 · 失败",
    "gray": "灰 · 未跑/不全",
}

# ---------------------------------------------------------------------------
# 考卷字段提取(逐行字面量,见模块顶部说明——不是 TOML 解析)
# ---------------------------------------------------------------------------

_KV_RE = re.compile(r'^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$')


def _parse_quoted(rest):
    """rest 是 `key =` 之后 strip 过的文本。只接受单行、两端 ASCII 双引号、
    内部不含 ASCII 双引号的字符串字面量——这是我们自己手写考卷文件的唯一
    形状。不符合就返回 (None, False),调用方负责"如实报错",这里绝不猜。
    """
    if len(rest) < 2 or rest[0] != '"' or rest[-1] != '"':
        return None, False
    inner = rest[1:-1]
    if '"' in inner:
        return None, False
    return inner, True


def scan_flow_toml(text):
    """逐行字面量提取 name/purpose/[[verify]] 的 sql+expect/[[step]] 的可选
    snap。返回 (fields, errors)。fields 里 verifies 只包含"sql 和 expect
    都提取成功"的完整块;errors 是人可读的提取失败原因列表,每条都带着
    出问题的原始行文本。绝不静默丢字段、绝不静默猜值。
    """
    fields = {"name": None, "purpose": None, "verifies": [], "snaps": []}
    errors = []
    section = "top"  # "top" | "step" | "verify" | "other"
    current = {}
    had_error_in_block = False

    def flush():
        nonlocal current, had_error_in_block
        if section == "verify":
            has_sql = "sql" in current
            has_expect = "expect" in current
            if has_sql and has_expect:
                fields["verifies"].append(
                    {"sql": current["sql"], "expect": current["expect"]}
                )
            elif not had_error_in_block:
                missing = []
                if not has_sql:
                    missing.append("sql")
                if not has_expect:
                    missing.append("expect")
                errors.append(
                    "考卷字段提取失败([[verify]] 块缺 " + "/".join(missing) + ")"
                )
        current = {}
        had_error_in_block = False

    for raw in text.splitlines():
        stripped = raw.strip()
        if stripped.startswith("[["):
            flush()
            if stripped == "[[verify]]":
                section = "verify"
            elif stripped == "[[step]]":
                section = "step"
            else:
                section = "other"
            continue

        m = _KV_RE.match(stripped)
        if not m:
            continue
        key = m.group(1)
        rest = m.group(2).strip()

        if section == "top" and key in ("name", "purpose") and fields[key] is None:
            value, ok = _parse_quoted(rest)
            if ok:
                fields[key] = value
            else:
                errors.append(f"考卷字段提取失败:{raw.strip()}")
        elif section == "verify" and key in ("sql", "expect"):
            value, ok = _parse_quoted(rest)
            if ok:
                current[key] = value
            else:
                had_error_in_block = True
                errors.append(f"考卷字段提取失败:{raw.strip()}")
        elif section == "step" and key == "snap":
            value, ok = _parse_quoted(rest)
            if ok:
                fields["snaps"].append(value)
            else:
                errors.append(f"考卷字段提取失败:{raw.strip()}")
        # 其余 key(do/arg/wait_s/db/launch/...)与本报告无关,原样忽略。

    flush()
    return fields, errors


# ---------------------------------------------------------------------------
# cmds.log 解析(主证据——逐行,永远原样展示,绝不接受人工转录的中间格式)
# ---------------------------------------------------------------------------

_LOG_LINE_RE = re.compile(r"^\[BW_FLOW\]\s+(\d+)\s+(.*)$")


def parse_cmds_log(text):
    """按 `[BW_FLOW] <n> ` 前缀 + ` ok`/` fail: ` 后缀解析,不做朴素的按空
    白切分(命令原文本身含空格与中文标点)。返回 (entries, unparsable_lines)。
    entries: [{"n": int, "cmd": str, "ok": bool, "reason": str|None}]
    unparsable_lines: 匹配了 `[BW_FLOW] <n> ` 前缀、但既不以 ` ok` 结尾也没
    有 ` fail: ` 标记的行——这种行不可信,原样记下来,绝不当成 ok。
    """
    entries = []
    unparsable = []
    for raw in text.splitlines():
        if not raw.strip():
            continue
        m = _LOG_LINE_RE.match(raw)
        if not m:
            continue
        n = int(m.group(1))
        rest = m.group(2)
        if rest.endswith(" ok"):
            entries.append({"n": n, "cmd": rest[:-3], "ok": True, "reason": None})
            continue
        idx = rest.rfind(" fail: ")
        if idx != -1:
            entries.append(
                {
                    "n": n,
                    "cmd": rest[:idx],
                    "ok": False,
                    "reason": rest[idx + len(" fail: ") :],
                }
            )
            continue
        unparsable.append(raw)
    return entries, unparsable


def load_readback(path):
    """读 readback.jsonl,一行一个 {"sql":..., "raw":...}。容错但不静默:
    非法 JSON 或缺字段的行不计入 readback 条数,原样记进 issues。
    """
    entries = []
    issues = []
    if not path.is_file():
        return entries, issues
    for i, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw.strip():
            continue
        try:
            obj = json.loads(raw)
        except json.JSONDecodeError:
            issues.append(f"readback.jsonl 第 {i} 行不是合法 JSON:{raw.strip()}")
            continue
        sql = obj.get("sql")
        rawout = obj.get("raw")
        if sql is None or rawout is None:
            issues.append(f"readback.jsonl 第 {i} 行缺 sql 或 raw 字段:{raw.strip()}")
            continue
        entries.append({"sql": sql, "raw": rawout})
    return entries, issues


# ---------------------------------------------------------------------------
# 三态判定(如实,绝不假绿——见 brief 设计约束 3)
# ---------------------------------------------------------------------------


def compute_flow_status(log_entries, log_unparsable, readback_count, verify_count, exam_present, data_issues):
    if not log_entries and not log_unparsable:
        return "gray", ["cmds.log 缺失或为空(未跑)"]

    fails = [e for e in log_entries if not e["ok"]]
    if fails:
        reasons = [f'第 {e["n"]} 行 fail: {e["reason"]}' for e in fails]
        reasons += [f"cmds.log 有无法解析的行:{raw}" for raw in log_unparsable]
        return "red", reasons

    if log_unparsable:
        return "gray", [f"cmds.log 有无法解析的行,不可全信:{raw}" for raw in log_unparsable]

    if data_issues:
        return "gray", list(data_issues)

    if exam_present and readback_count < verify_count:
        missing_n = verify_count - readback_count
        return "gray", [
            f"未读回:考卷 {verify_count} 条 [[verify]],实际只有 {readback_count} 条 sqlite 读回,缺 {missing_n} 条"
        ]

    return "green", []


def compute_batch_status(statuses):
    if not statuses:
        return "gray"
    if "red" in statuses:
        return "red"
    if "gray" in statuses:
        return "gray"
    if all(s == "green" for s in statuses):
        return "green"
    return "gray"


# ---------------------------------------------------------------------------
# 单条流的证据装配
# ---------------------------------------------------------------------------


def build_flow_report(flow_dir, repo_root):
    dir_name = flow_dir.name

    cmds_log_path = flow_dir / "cmds.log"
    log_text = cmds_log_path.read_text(encoding="utf-8") if cmds_log_path.is_file() else ""
    log_entries, log_unparsable = parse_cmds_log(log_text)

    readback_entries, readback_issues = load_readback(flow_dir / "readback.jsonl")

    exam_primary = flow_dir / "flow.toml"
    exam_fallback = repo_root / "e2e" / "flows" / "core" / f"{dir_name}.toml"
    if exam_primary.is_file():
        exam_source = exam_primary
    elif exam_fallback.is_file():
        exam_source = exam_fallback
    else:
        exam_source = None

    if exam_source is not None:
        fields, extraction_errors = scan_flow_toml(exam_source.read_text(encoding="utf-8"))
    else:
        fields, extraction_errors = (
            {"name": None, "purpose": None, "verifies": [], "snaps": []},
            [],
        )
    exam_present = exam_source is not None

    data_issues = list(extraction_errors) + list(readback_issues)

    snaps_dir = flow_dir / "snaps"
    actual_snaps = {}
    if snaps_dir.is_dir():
        for p in sorted(snaps_dir.glob("*.png")):
            actual_snaps[p.stem] = p
    snaps_nonempty = snaps_dir.is_dir() and any(snaps_dir.iterdir())

    status, reasons = compute_flow_status(
        log_entries=log_entries,
        log_unparsable=log_unparsable,
        readback_count=len(readback_entries),
        verify_count=len(fields["verifies"]),
        exam_present=exam_present,
        data_issues=data_issues,
    )

    return {
        "dir_name": dir_name,
        "status": status,
        "reasons": reasons,
        "exam_present": exam_present,
        "exam_source": str(exam_source) if exam_source else None,
        "toml_name": fields["name"],
        "purpose": fields["purpose"],
        "verifies": fields["verifies"],
        "log_entries": log_entries,
        "log_unparsable": log_unparsable,
        "readback_entries": readback_entries,
        "expected_snap_names": fields["snaps"],
        "actual_snaps": actual_snaps,
        "snaps_nonempty": snaps_nonempty,
    }


def load_meta(run_dir):
    meta_path = run_dir / "meta.json"
    if not meta_path.is_file():
        return {"batch": "(meta.json 缺失)", "started_at": "?", "commit": "?"}
    try:
        obj = json.loads(meta_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {"batch": "(meta.json 不是合法 JSON)", "started_at": "?", "commit": "?"}
    return {
        "batch": obj.get("batch", "?"),
        "started_at": obj.get("started_at", "?"),
        "commit": obj.get("commit", "?"),
    }


# ---------------------------------------------------------------------------
# HTML 渲染(单文件、无外链、无 JS 依赖;宽内容自身横向滚动,页面本身不滚动)
# ---------------------------------------------------------------------------


def esc(value):
    return html.escape(str(value), quote=True)


def badge(status):
    return f'<span class="badge badge-{status}">{esc(STATUS_LABEL[status])}</span>'


def render_log_table(flow):
    if not flow["log_entries"] and not flow["log_unparsable"]:
        return '<p class="note-missing">cmds.log 缺失或为空(未跑)。</p>'
    rows = []
    for e in flow["log_entries"]:
        result = (
            f'<span style="color:{GREEN}">ok</span>'
            if e["ok"]
            else f'<span style="color:{RED}">fail</span>'
        )
        reason = esc(e["reason"]) if e["reason"] is not None else ""
        rows.append(
            "<tr>"
            f'<td>{e["n"]}</td>'
            f'<td class="mono">{esc(e["cmd"])}</td>'
            f"<td>{result}</td>"
            f'<td class="mono">{reason}</td>'
            "</tr>"
        )
    for raw in flow["log_unparsable"]:
        rows.append(
            "<tr>"
            f'<td colspan="4" class="mono" style="color:{RED}">'
            f"无法解析的日志行(原样展示,不当成 ok):{esc(raw)}</td>"
            "</tr>"
        )
    body = "\n".join(rows)
    return (
        '<div class="scroll-x"><table class="log">'
        "<tr><th>#</th><th>命令(原样)</th><th>结果</th><th>原因(fail 时)</th></tr>"
        f"{body}"
        "</table></div>"
    )


def render_verify_table(flow):
    verifies = flow["verifies"]
    readback = flow["readback_entries"]

    if not flow["exam_present"]:
        if not readback:
            return '<p class="note-missing">无 sqlite 读回记录。</p>'
        rows = []
        for rb in readback:
            rows.append(
                "<tr>"
                f'<td class="mono">{esc(rb["sql"])}</td>'
                f'<td class="mono">{esc(rb["raw"])}</td>'
                '<td class="note-missing">N/A(考卷缺失,无法对照 expect)</td>'
                "</tr>"
            )
        body = "\n".join(rows)
        return (
            '<div class="scroll-x"><table class="verify">'
            "<tr><th>SQL 语句</th><th>原始输出(sqlite3)</th><th>预期陈述(人读,非机器判定)</th></tr>"
            f"{body}"
            "</table></div>"
        )

    if not verifies:
        return '<p class="note-missing">考卷未能提取出任何完整的 [[verify]] 字段(见上方提取失败原因)。</p>'

    rows = []
    for i, v in enumerate(verifies):
        if i < len(readback):
            raw_cell = f'<td class="mono">{esc(readback[i]["raw"])}</td>'
        else:
            raw_cell = f'<td class="mono note-missing">未读回</td>'
        rows.append(
            "<tr>"
            f'<td class="mono">{esc(v["sql"])}</td>'
            f"{raw_cell}"
            f'<td>{esc(v["expect"])}</td>'
            "</tr>"
        )
    body = "\n".join(rows)
    return (
        '<div class="scroll-x"><table class="verify">'
        "<tr><th>SQL 语句</th><th>原始输出(sqlite3)</th><th>预期陈述(人读,非机器判定)</th></tr>"
        f"{body}"
        "</table></div>"
    )


def render_snaps(flow):
    if not flow["snaps_nonempty"]:
        return '<p class="note-missing">本次运行未取得截图(环境未授权屏幕录制)。</p>'

    actual = flow["actual_snaps"]
    parts = ['<div class="snaps">']
    seen = set()
    for name in flow["expected_snap_names"]:
        seen.add(name)
        if name in actual:
            b64 = base64.b64encode(actual[name].read_bytes()).decode("ascii")
            parts.append(
                '<figure class="snap-figure">'
                f'<img class="snap" src="data:image/png;base64,{b64}" alt="{esc(name)}">'
                f"<figcaption>{esc(name)}</figcaption>"
                "</figure>"
            )
        else:
            parts.append(f'<p class="note-missing">缺图:{esc(name)}</p>')
    for name, p in actual.items():
        if name in seen:
            continue
        b64 = base64.b64encode(p.read_bytes()).decode("ascii")
        parts.append(
            '<figure class="snap-figure">'
            f'<img class="snap" src="data:image/png;base64,{b64}" alt="{esc(name)}">'
            f"<figcaption>{esc(name)}(考卷未声明此名,附上供参考)</figcaption>"
            "</figure>"
        )
    parts.append("</div>")
    return "\n".join(parts)


def render_flow_section(flow):
    reasons_html = ""
    if flow["reasons"]:
        items = "".join(f"<li>{esc(r)}</li>" for r in flow["reasons"])
        reasons_html = f'<ul class="reasons">{items}</ul>'

    if flow["exam_present"]:
        purpose_html = f'<p class="purpose">{esc(flow["purpose"]) if flow["purpose"] else "(purpose 提取失败,见下)"}</p>'
        exam_note = f'<p class="exam-source">考卷:<span class="path">{esc(flow["exam_source"])}</span></p>'
    else:
        purpose_html = '<p class="note-missing">考卷缺失,无法对照 purpose/expect。</p>'
        exam_note = ""

    return f"""
<section class="flow-section" id="flow-{esc(flow['dir_name'])}">
  <h2>{badge(flow['status'])} {esc(flow['dir_name'])}</h2>
  {exam_note}
  {purpose_html}
  {reasons_html}
  <h3>驱动日志(cmds.log · 主证据,逐行原样)</h3>
  {render_log_table(flow)}
  <h3>SQL 读回核验([[verify]])</h3>
  {render_verify_table(flow)}
  <h3>截图</h3>
  {render_snaps(flow)}
</section>
"""


CSS = f"""
:root {{
  --paper: {PAPER};
  --clay: {CLAY};
  --ink: {INK};
  --green: {GREEN};
  --red: {RED};
  --gray: {GRAY};
}}
* {{ box-sizing: border-box; }}
html, body {{
  margin: 0; padding: 0;
  background: var(--paper);
  color: var(--ink);
  overflow-x: hidden;
}}
body {{
  font-family: "Noto Sans SC", "PingFang SC", "Helvetica Neue", Arial, sans-serif;
  padding: 24px 32px 64px;
  max-width: 1100px;
  margin: 0 auto;
  line-height: 1.55;
}}
h1, h2, h3 {{
  font-family: "Noto Serif SC", "Songti SC", "Georgia", serif;
  color: var(--ink);
}}
h1 {{ border-bottom: 3px solid var(--clay); padding-bottom: 8px; }}
h2 {{ margin-top: 0; }}
.mono {{
  font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  white-space: pre;
}}
.badge {{
  display: inline-block;
  padding: 2px 12px;
  border-radius: 4px;
  font-weight: 700;
  color: #fff;
  font-size: 0.9em;
}}
.badge-green {{ background: var(--green); }}
.badge-red {{ background: var(--red); }}
.badge-gray {{ background: var(--gray); }}
.scroll-x {{ overflow-x: auto; max-width: 100%; margin: 8px 0; }}
table {{ border-collapse: collapse; width: max-content; min-width: 100%; }}
th, td {{ border: 1px solid #d8d2c4; padding: 6px 10px; text-align: left; vertical-align: top; overflow-wrap: anywhere; }}
th {{ background: #e6ded0; }}
.flow-section {{
  margin: 28px 0;
  padding: 16px 20px;
  background: #fff8ef;
  border: 1px solid #ddd5c2;
  border-radius: 6px;
}}
.reasons {{ color: var(--red); overflow-wrap: anywhere; }}
.note-missing {{ color: var(--gray); font-style: italic; }}
.purpose {{ font-style: italic; }}
.exam-source {{ color: var(--gray); font-size: 0.85em; }}
.path {{
  font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  white-space: normal;
  word-break: break-all;
}}
.snaps {{ display: flex; flex-wrap: wrap; gap: 12px; }}
.snap-figure {{ margin: 0; max-width: 480px; }}
.snap {{ max-width: 100%; height: auto; border: 1px solid #ccc; display: block; }}
figcaption {{ font-size: 0.85em; color: var(--gray); margin-top: 4px; }}
.batch-header {{ margin-bottom: 24px; }}
"""


def render_html(meta, batch_status, flow_reports):
    flows_html = "\n".join(render_flow_section(f) for f in flow_reports)
    if not flow_reports:
        flows_html = '<p class="note-missing">本批次未发现任何流目录(flows/ 缺失或为空)。</p>'

    return f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>验收证据报告 · {esc(meta['batch'])}</title>
<style>{CSS}</style>
</head>
<body>
<header class="batch-header">
  <h1>验收证据报告</h1>
  <p>批次:<strong>{esc(meta['batch'])}</strong> · 起跑:{esc(meta['started_at'])} · commit:<span class="mono">{esc(meta['commit'])}</span></p>
  <p>总态:{badge(batch_status)}</p>
</header>
{flows_html}
</body>
</html>
"""


# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------


def main(argv):
    if len(argv) != 2:
        print("用法: python3 scripts/gen-flow-report.py <run-dir>", file=sys.stderr)
        return 2

    run_dir = Path(argv[1]).resolve()
    if not run_dir.is_dir():
        print(f"run-dir 不存在或不是目录: {run_dir}", file=sys.stderr)
        return 2

    meta = load_meta(run_dir)
    repo_root = Path(__file__).resolve().parent.parent

    flows_root = run_dir / "flows"
    flow_dirs = sorted(p for p in flows_root.iterdir() if p.is_dir()) if flows_root.is_dir() else []

    flow_reports = [build_flow_report(fd, repo_root) for fd in flow_dirs]
    batch_status = compute_batch_status([f["status"] for f in flow_reports])

    out_path = run_dir / "report.html"
    out_path.write_text(render_html(meta, batch_status, flow_reports), encoding="utf-8")
    print(str(out_path))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
