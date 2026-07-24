# 验收动作流基建 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地 plan/15:第 0 阶段三关闸门实测 → 验收基建(打包脚本、五条常青考卷、证据报告生成器)→ 首次验收跑产出 report.html 交用户终审。

**Architecture:** 零 Rust 改动。BW.app 手工骨架包装既有 debug 二进制;考卷是 TOML 语义步骤,由 Fable 用 computer-use 亲手驾驶执行;每步证据(PNG + steps.jsonl + sqlite 读回)落 `e2e/reports/`,python 生成器装配单文件 HTML 报告。

**Tech Stack:** bash / TOML / python3(标准库) / computer-use MCP / sqlite3 / screencapture。

## Global Constraints

- **不写单元测试**(2026-07-17 纪律);验证 = 实跑 + 读回。
- **E2E 绝不依赖网关**(CLAUDE.md 纪律 3):考卷 ①-④ 全走 Mock 执行器路径。
- **原库绝不触碰**:fixture 与 `~/Library/Application Support/BuildersWorkbench/workbench.db` 只被复制,流跑在副本上。
- **失败如实**:verdict 只有 ok/fail/skipped;fail 停在原地,后续步全记 skipped;绝不补拍。
- **闸门纪律**:Task 2 三关不全过,Task 3-6 不开工;卡死拿证据找用户,不静默换路线。
- 本批零 Rust 改动;提交前跑一遍完整门禁以证清白(不绿即环境坏,与本批无关也要报)。
- commit 带代号前缀 `V1-<n>`,信息如实,末尾 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`。
- **Fable-only 任务**(需 computer-use 交互,不派 subagent):Task 2、Task 6。

---

### Task 1: BW.app 打包脚本(G-1 载体)

**Files:**
- Create: `scripts/bundle-desktop.sh`
- Modify: `.gitignore`(追加 `dist/`)

**Interfaces:**
- Produces: `dist/BW.app`,启动方式 `BW_DB=… dist/BW.app/Contents/MacOS/builders-workbench`(终端直启保留 env;`open` 不传 env 所以不用)。

**偏差记录(写进 commit message):** plan/15 G-1 原文写 `dx bundle`,实测本机 `dx` 未安装;改为手工 .app 骨架(意图不变:窗口成为 OS 一等公民)。若 Task 2 G-1 验证失败,再回头 `cargo install dioxus-cli` 走 dx bundle 二次尝试。

- [ ] **Step 1: 写脚本**

```bash
#!/usr/bin/env bash
# scripts/bundle-desktop.sh — 把 debug 二进制包成 dist/BW.app(plan/15 G-1)。
# 启动必须终端直启 Contents/MacOS/ 下的二进制以保留 BW_* env 深链。
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p app-desktop
APP=dist/BW.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/debug/builders-workbench "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>builders-workbench</string>
  <key>CFBundleIdentifier</key><string>com.buildersworkbench.desktop</string>
  <key>CFBundleName</key><string>Builders' Workbench</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "[bundle] ready: BW_DB=<db> $APP/Contents/MacOS/builders-workbench"
```

- [ ] **Step 2: 跑通 + 深链冒烟**

```bash
chmod +x scripts/bundle-desktop.sh && ./scripts/bundle-desktop.sh
BW_DB=$(mktemp -d)/t.db BW_HUB=workflow dist/BW.app/Contents/MacOS/builders-workbench 2>&1 | head -5 &
sleep 8 && pkill -f 'BW.app/Contents/MacOS/builders-workbench'
```
Expected: stderr 出现 `[BW_HUB] "workflow" -> Workflow`(渲染证明,老纪律)。

- [ ] **Step 3: .gitignore 追加 `dist/`;完整门禁;commit `V1-1 · BW.app 手工骨架打包脚本(dx 缺席,偏差如实)`**

---

### Task 2: 三关闸门实测(Fable-only,交互式)

**Files:**
- Create: `iterations/evidence/gate-2026-07-24/GATE.md`(+ 三关证据 PNG)

**Interfaces:**
- Consumes: Task 1 的 `dist/BW.app`。
- Produces: go/no-go 裁决。**任何一关 fail:停,带证据找用户,Task 3-6 不开工。**

- [ ] **Step 1(G-1 窗口):** 启动 BW.app(临时 db + `BW_OPEN`);`mcp__computer-use__request_access` 申请 BW 应用;MCP `screenshot` 确认窗口可见。证据:截图存 `iterations/evidence/gate-2026-07-24/g1-window.png`(MCP 图不可落盘则先用 G-3 的 screencapture 补拍,如实注明)。
- [ ] **Step 2(G-2 点击):** MCP 点击左侧图标栏一个 Hub 图标,前后各一张截图,肉眼可见屏幕切换。证据:`g2-before.png` / `g2-after.png`。
- [ ] **Step 3(G-3 落盘):** 枚举窗口 ID 后 `screencapture -l<ID> g3-capture.png`:

```bash
swift - <<'EOF'
import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
for w in list where (w[kCGWindowOwnerName as String] as? String ?? "").lowercased().contains("builders") {
  print(w[kCGWindowNumber as String]!, w[kCGWindowName as String] as? String ?? "?")
}
EOF
```
拍到的是墙纸/空白 = 终端宿主缺「屏幕录制」权限 → **请用户去 系统设置→隐私与安全性→屏幕录制 给终端宿主授权一次**,重测。
- [ ] **Step 4:** 写 `GATE.md`(三关各:做了什么/证据文件/verdict);commit `V1-2 · 三关闸门实测证据`。

---

### Task 3: e2e 脚手架 + fixture 种子库

**Files:**
- Create: `e2e/fixtures/README.md`, `e2e/fixtures/demo.db`(生成物,进 git), `scripts/flow-prep.sh`
- Modify: `.gitignore`(追加 `e2e/reports/`)

**Interfaces:**
- Produces: `scripts/flow-prep.sh <fixture:demo|fixture:empty|copy:daily> <run-dir>` → 打印临时 db 路径(供 `BW_DB=` 注入)。fixture/原库只被 `cp`,绝不直接跑。

- [ ] **Step 1: 生成种子库**

```bash
mkdir -p e2e/fixtures
cargo run -p bw-app --example real_demo -- e2e/fixtures/demo.db "$(mktemp -d)" --mock
sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM project; SELECT status, count(*) FROM issue GROUP BY status;"
```
Expected: project ≥1;issue 含 Done 与非 Done 状态(蒸馏考卷需要 Done,跑单考卷需要可开工的卡)。数字如实抄进 README。

- [ ] **Step 2: 写 `scripts/flow-prep.sh`**

```bash
#!/usr/bin/env bash
# 用法: flow-prep.sh <fixture:demo|fixture:empty|copy:daily> <run-dir> → stdout 打临时 db 路径
set -euo pipefail
SRC_KIND="$1"; RUN_DIR="$2"; mkdir -p "$RUN_DIR/tmp"
DB="$RUN_DIR/tmp/flow-$(date +%s).db"
case "$SRC_KIND" in
  fixture:demo)  cp "$(dirname "$0")/../e2e/fixtures/demo.db" "$DB" ;;
  fixture:empty) : ;;   # 不建文件,app 首启自建 schema
  copy:daily)    cp "$HOME/Library/Application Support/BuildersWorkbench/workbench.db" "$DB" ;;
  *) echo "unknown source: $SRC_KIND" >&2; exit 1 ;;
esac
echo "$DB"
```

- [ ] **Step 3: 冒烟 `flow-prep.sh fixture:demo /tmp/x` 出路径且文件存在;README 写来源/再生成命令/只跑副本纪律;.gitignore 追加;门禁;commit `V1-3 · e2e 脚手架与 fixture 种子库`**

---

### Task 4: 五条常青考卷

**Files:**
- Create: `e2e/flows/core/01-create-project.toml` ~ `05-daily-smoke.toml`

**Interfaces:**
- Consumes: `flow-prep.sh` 的 db 注入;真实 UI 文案。
- Produces: Fable 执行用考卷。步骤字段:`do`(click/type/wait/snap/quit)、`where`/`until`(语义定位)、`snap`(截图名)、`timeout_s`;流级字段:`name`/`purpose`/`db`/`launch`(env 表)/`[[verify]]`(sql+expect)。

- [ ] **Step 1: 对照源码校准文案**(考卷 `where` 必须用真实按钮文案):`wall.rs`(新建项目)、`create.rs`(创建表单各步文案——**注意**:核查创建路径是否会触发真执行器起草;若会,考卷 launch 加 `BW_CLAUDE_BIN=<stub>` 指向自答脚本并在 purpose 里如实标注「stub CLI」,绝不让考卷依赖网关)、`op.rs`(`▶ 跑`/`✓ 确认完成(人裁)`/`⚗ 蒸馏为技能`/`确认蒸馏`)、`chrome.rs`(图标栏悬浮文案)。
- [ ] **Step 2: 写五份考卷**(骨架如下,02 为完整示例,其余同构):

```toml
# e2e/flows/core/02-issue-run-review-done.toml
name    = "issue-run-review-done"
purpose = "铁律:跑 Issue 只推评审中,Done 永远人点,settled_at 落账"
db      = "fixture:demo"
launch  = { BW_PANEL = "issues" }   # BW_OPEN 项目名执行时从 fixture 读回后填注

[[step]]
do = "snap"
snap = "issues-initial"

[[step]]
do    = "click"
where = "一张「未开工」Issue 卡上的 ▶ 跑 按钮"
snap  = "run-clicked"

[[step]]
do        = "wait"
until     = "该卡状态徽记变为「评审中」"
timeout_s = 90

[[verify]]
sql    = "SELECT status, settled_at FROM issue WHERE id='<执行时读回填注>'"
expect = "InReview 且 settled_at 为 NULL(未人点,绝不自动 Done)"

[[step]]
do    = "click"
where = "该评审中卡片的 ✓ 确认完成(人裁) 按钮"
snap  = "human-done"

[[verify]]
sql    = "SELECT status, settled_at FROM issue WHERE id='<执行时读回填注>'"
expect = "Done 且 settled_at 非空"
```

五条覆盖(plan/15 §4):01 建项目(db=fixture:empty)/ 02 如上 / 03 人点 Done 独立卡(与 02 不同卡,验 Done 入边仅 InReview)/ 04 蒸馏(fixture 已有 Done 卡→⚗ 蒸馏为技能→确认蒸馏→verify skill 行+来源归属)/ 05 冒烟(db=copy:daily,依次点 Workflow/Skill/Agent/Cron Hub 各 snap 一张,verify 若干 count 对照屏显)。
- [ ] **Step 3: 门禁;commit `V1-4 · 五条常青考卷`**

---

### Task 5: 证据报告生成器

**Files:**
- Create: `scripts/gen-flow-report.py`

**Interfaces:**
- Consumes(执行时由 Fable 逐步追加,schema 即契约):
  - `<run-dir>/steps.jsonl`:`{"flow":"02-issue-run-review-done","step":2,"do":"click","where":"…","verdict":"ok|fail|skipped","ms":1240,"snap_before":"02/step-02-before.png","snap_after":"02/step-02-after.png","note":""}`
  - `<run-dir>/readback.jsonl`:`{"flow":"…","sql":"…","raw":"…","expect":"…","verdict":"ok|fail"}`
  - 截图 PNG 相对 run-dir。
- Produces: `<run-dir>/report.html`(单文件,PNG base64 内嵌)。

- [ ] **Step 1: 写生成器**(python3 标准库;流分节,步骤表格含前后截图、verify 三列「SQL/原值/预期」,顶部三态汇总:绿=全 ok/红=任一 fail/灰=任一 skipped 或环境中断;暖纸底 `#EFEBE2`、clay `#C5654A`、fail 红、skipped 灰,plan/00 §6):

```python
#!/usr/bin/env python3
"""gen-flow-report.py <run-dir> — 装配单文件 HTML 证据报告(plan/15 §5)。"""
import base64, html, json, sys
from pathlib import Path

def load(p):
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []

def img(run, rel):
    f = run / rel
    if not f.exists():
        return f'<em class="miss">缺图:{html.escape(rel)}</em>'
    b = base64.b64encode(f.read_bytes()).decode()
    return f'<img src="data:image/png;base64,{b}" alt="{html.escape(rel)}">'

def main(run):
    steps, reads = load(run / "steps.jsonl"), load(run / "readback.jsonl")
    flows = {}
    for s in steps: flows.setdefault(s["flow"], {"steps": [], "reads": []})["steps"].append(s)
    for r in reads: flows.setdefault(r["flow"], {"steps": [], "reads": []})["reads"].append(r)
    def state(f):
        vs = [x["verdict"] for x in f["steps"] + f["reads"]]
        return "fail" if "fail" in vs else ("skipped" if "skipped" in vs else "ok")
    C = {"ok": "#4a7c59", "fail": "#b0413e", "skipped": "#8a8578"}
    parts = ["""<!doctype html><meta charset="utf-8"><title>BW 验收证据报告</title><style>
body{background:#EFEBE2;color:#2b2622;font-family:'Noto Sans SC',sans-serif;max-width:1080px;margin:2em auto;padding:0 1em}
h1{color:#C5654A}img{max-width:480px;border:1px solid #c9c2b4;display:block;margin:4px 0}
table{border-collapse:collapse;width:100%}td,th{border:1px solid #c9c2b4;padding:6px;vertical-align:top;text-align:left}
.b{display:inline-block;padding:2px 10px;border-radius:4px;color:#fff}code{background:#e5dfd2;padding:1px 4px}.miss{color:#b0413e}
</style><h1>BW 验收证据报告</h1>"""]
    overall = "ok" if flows and all(state(f) == "ok" for f in flows.values()) else ("fail" if any(state(f) == "fail" for f in flows.values()) else "skipped")
    parts.append(f'<p>批次:{html.escape(run.name)} · 总态 <span class="b" style="background:{C[overall]}">{overall}</span></p>')
    for name, f in flows.items():
        st = state(f)
        parts.append(f'<h2>{html.escape(name)} <span class="b" style="background:{C[st]}">{st}</span></h2><table><tr><th>#</th><th>动作</th><th>verdict</th><th>证据</th></tr>')
        for s in f["steps"]:
            shots = "".join(img(run, s[k]) for k in ("snap_before", "snap_after") if s.get(k))
            parts.append(f'<tr><td>{s["step"]}</td><td>{html.escape(s["do"])} · {html.escape(s.get("where", s.get("note", "")))}</td>'
                         f'<td style="color:{C[s["verdict"]]}">{s["verdict"]} · {s.get("ms", "?")}ms</td><td>{shots}</td></tr>')
        parts.append("</table>")
        if f["reads"]:
            parts.append('<table><tr><th>SQL</th><th>原值</th><th>预期陈述</th><th>verdict</th></tr>')
            parts.extend(f'<tr><td><code>{html.escape(r["sql"])}</code></td><td><code>{html.escape(r["raw"])}</code></td>'
                         f'<td>{html.escape(r["expect"])}</td><td style="color:{C[r["verdict"]]}">{r["verdict"]}</td></tr>' for r in f["reads"])
            parts.append("</table>")
    (run / "report.html").write_text("".join(parts))
    print(f"[report] {run/'report.html'}")

if __name__ == "__main__":
    main(Path(sys.argv[1]))
```

- [ ] **Step 2: 合成样例冒烟**:手造 `e2e/reports/sample/`(两行 steps.jsonl、一行 readback.jsonl、一张任意 PNG)→ 跑生成器 → `report.html` 存在且含 base64 图与三态徽记(grep 核对)。用后删除样例目录。
- [ ] **Step 3: 门禁;commit `V1-5 · 证据报告生成器`**

---

### Task 6: 首次验收跑(Fable-only,交互式)

**Files:**
- Create: `e2e/reports/<UTC时间戳>-first-acceptance/…`(gitignored)→ 终审通过后 `report.html` 复制 `iterations/evidence/acceptance-2026-07-24/` 进 git。

**Interfaces:**
- Consumes: Task 1-5 全部。
- Produces: 首份 `report.html` SendUserFile 交付;plan/15 DoD 勾除。

- [ ] **Step 1:** 逐条跑五考卷:`flow-prep.sh` 备库 → env 深链启动 BW.app → 按执行协议(截图→定位→动作→截图→对照→记 verdict)驾驶,每步即时追加 `steps.jsonl`,snap 用 `screencapture -l<ID>` 落盘 → 退出 app → `sqlite3` 读回追加 `readback.jsonl`。fail 停在原地,后续步记 skipped,如实进报告。
- [ ] **Step 2:** `gen-flow-report.py` 出报告,SendUserFile 送用户;stderr 全量日志留 run-dir。
- [ ] **Step 3(用户终审):** 用户看报告点头 = 工作流第④步走通;归档报告进 `iterations/evidence/`;commit `V1-6 · 首次验收跑证据归档`;勾 plan/15 §10 DoD;offer commit-push-pr。

---

## Self-Review 备忘

- 覆盖:plan/15 §2 三关→Task 1/2;§3 考卷→Task 4;§4 五条→Task 4;§5 协议+报告→Task 5/6;§10 DoD→Task 6。§6 工作流本体:本批以「Task 3-5 可派 Sonnet、Task 2/6 Fable 亲驾、用户终审」的执行方式自证,不另立任务。
- 类型一致:steps.jsonl/readback.jsonl schema 在 Task 5 Interfaces 定义,Task 6 消费同 schema;flow-prep.sh 签名 Task 3 定义、Task 6 消费。
- 无占位:考卷中 `<执行时读回填注>` 是执行期动作(从 fixture 读回真实 id/项目名后填入),非 TBD——考卷绝不硬编 fixture 里的 uuid,防种子库再生成后烂掉。
