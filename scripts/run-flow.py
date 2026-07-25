#!/usr/bin/env python3
"""按考卷实跑一条验收流,产出机器证据到 run-dir。

    python3 scripts/run-flow.py e2e/flows/core/02-issue-run-to-review.toml <run-dir>

**为什么必须由本脚本照考卷跑,而不是人手敲命令**:考卷是验收件。若实跑命令
由操作者临时敲入,真正被验的就是"操作者敲了什么",考卷沦为装饰,二者会静默
漂移。本脚本是那道防漂移闸:动作序列只从考卷来。

不解析 TOML(本机 python3=3.9,无 tomllib;拒绝引入依赖或手写通用解析器,
理由同 scripts/gen-flow-report.py)。改为对**我们自己撰写**的考卷格式做定向
逐行提取,任何提取不了的行**立即报错退出**,绝不猜、绝不静默跳过。

产出(与 gen-flow-report.py 的输入契约一致):
    <run-dir>/flows/<考卷名>/flow.toml        考卷副本(证据自足)
    <run-dir>/flows/<考卷名>/cmds             实际下发的命令(人可读)
    <run-dir>/flows/<考卷名>/cmds.log         驱动自产结果日志(主证据)
    <run-dir>/flows/<考卷名>/readback.jsonl   每行 {"sql","raw"},sqlite3 原始输出
    <run-dir>/flows/<考卷名>/snaps/           截图(考卷 step 带 snap 字段时,本脚本下发对应
                                               `snap <name>` 命令,由驱动器应用内自截并写出)
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "dist" / "BW.app" / "Contents" / "MacOS" / "builders-workbench"
BOOT_WAIT_S = 13
DEFAULT_WAIT_S = 2
SNAP_WAIT_S = 2  # 步骤自身 wait_s 之后再给 snap 命令的独立结算时间


def die(msg):
    print(f"[run-flow] 错误:{msg}", file=sys.stderr)
    sys.exit(1)


def unquote(raw, lineno, line):
    """取 `key = "值"` 的值。取不出就报错——绝不猜。"""
    v = raw.strip()
    if len(v) >= 2 and v.startswith('"') and v.endswith('"'):
        return v[1:-1]
    die(f"第 {lineno} 行字段值不是完整的双引号字符串,拒绝猜测:{line.rstrip()}")


def parse_launch(raw, lineno, line):
    """`launch = { BW_OPEN = "x", BW_PANEL = "y" }` → dict;`{}` → {}。"""
    v = raw.strip()
    if not (v.startswith("{") and v.endswith("}")):
        die(f"第 {lineno} 行 launch 不是内联表:{line.rstrip()}")
    inner = v[1:-1].strip()
    if not inner:
        return {}
    env = {}
    for pair in re.finditer(r'(\w+)\s*=\s*"([^"]*)"', inner):
        env[pair.group(1)] = pair.group(2)
    if not env:
        die(f"第 {lineno} 行 launch 内联表解析不出任何键值:{line.rstrip()}")
    return env


def parse_flow(path):
    """考卷 → {name, db, phases:[{name, launch, steps, verifies}]}。

    01-04 是单阶段(顶层 launch/[[step]]/[[verify]]);05 是多阶段
    ([[phase]] + [[phase.step]] + [[phase.verify]],每阶段各自 launch)。
    """
    meta = {"name": None, "db": None}
    top_launch = {}
    phases = []
    cur_phase = None
    cur_kind = None  # "step" | "verify"
    cur_item = None

    def flush_item():
        nonlocal cur_item, cur_kind
        if cur_item is None:
            return
        target = cur_phase if cur_phase is not None else implicit
        target["steps" if cur_kind == "step" else "verifies"].append(cur_item)
        cur_item, cur_kind = None, None

    implicit = {"name": "main", "launch": {}, "steps": [], "verifies": []}

    for lineno, line in enumerate(path.read_text().splitlines(), 1):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        if s.startswith("[["):
            flush_item()
            if s == "[[phase]]":
                cur_phase = {"name": None, "launch": {}, "steps": [], "verifies": []}
                phases.append(cur_phase)
            elif s in ("[[step]]", "[[phase.step]]"):
                cur_kind, cur_item = "step", {}
            elif s in ("[[verify]]", "[[phase.verify]]"):
                cur_kind, cur_item = "verify", {}
            else:
                die(f"第 {lineno} 行:未知区块 {s}")
            continue
        if "=" not in s:
            die(f"第 {lineno} 行既不是区块也不是键值对:{line.rstrip()}")
        key, raw = s.split("=", 1)
        key = key.strip()
        if key == "launch":
            env = parse_launch(raw, lineno, line)
            if cur_phase is not None and cur_item is None:
                cur_phase["launch"] = env
            else:
                top_launch.update(env)
            continue
        val = raw.strip()
        if key == "wait_s":
            num = val.strip()
            if not num.isdigit():
                die(f"第 {lineno} 行 wait_s 不是整数:{line.rstrip()}")
            cur_item["wait_s"] = int(num)
            continue
        text = unquote(raw, lineno, line)
        if cur_item is not None:
            cur_item[key] = text
        elif cur_phase is not None:
            cur_phase[key] = text
        elif key in meta:
            meta[key] = text
        # 其余顶层键(purpose 等)本脚本不需要,由报告生成器从考卷副本读。
    flush_item()

    if not meta["name"] or not meta["db"]:
        die("考卷缺 name 或 db")
    if not phases:
        implicit["launch"] = top_launch
        phases = [implicit]
    return meta, phases


def prep_db(db_kind, run_dir):
    out = subprocess.run(
        [str(ROOT / "scripts" / "flow-prep.sh"), db_kind, str(run_dir)],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        die(f"flow-prep.sh 失败:{out.stderr.strip()}")
    return out.stdout.strip()


def kill_app():
    """杀掉 app 并**确认它真的退了**再返回。

    实测教训:上一拍的实例若未退净,下一拍启动会拿到 SQLITE_CANTOPEN
    (界面进 FatalFrame「本地数据库打开失败」),报告上表现为一条**假红**。
    验收工具产生假红比假绿好不了多少 —— 都会让人不再信报告。所以这里必须
    轮询到进程消失为止,而不是睡固定秒数赌它退完。
    """
    pat = "BW.app/Contents/MacOS/builders-workbench"
    subprocess.run(["pkill", "-f", pat])
    for _ in range(30):  # 最多等 15s
        if subprocess.run(["pgrep", "-f", pat], capture_output=True).returncode != 0:
            time.sleep(1)  # 让 SQLite 句柄彻底释放
            return
        time.sleep(0.5)
    die("app 进程在 15s 内未退出,拒绝在脏状态下继续跑(否则会产生假红)")


def run_phase(phase, db_path, flow_dir, idx):
    """拉起一次 app,按序下发本阶段命令,回收日志。"""
    cmd_file = flow_dir / f"cmds-{idx:02d}-{phase.get('name') or 'main'}"
    cmd_file.write_text("")
    env = dict(os.environ)
    env["BW_DB"] = db_path
    env["BW_FLOW"] = str(cmd_file)
    env.update(phase["launch"])
    kill_app()
    proc = subprocess.Popen(
        [str(APP)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    time.sleep(BOOT_WAIT_S)
    lines = []
    for st in phase["steps"]:
        verb, arg = st.get("do"), st.get("arg", "")
        if not verb:
            die("考卷里有缺 do 的 step")
        line = f"{verb} {arg}".rstrip()
        lines.append(line)
        with cmd_file.open("a") as f:
            f.write(line + "\n")
        time.sleep(st.get("wait_s", DEFAULT_WAIT_S))
        snap_name = st.get("snap")
        if snap_name:
            snap_line = f"snap {snap_name}"
            lines.append(snap_line)
            with cmd_file.open("a") as f:
                f.write(snap_line + "\n")
            time.sleep(SNAP_WAIT_S)
    time.sleep(2)
    proc.terminate()
    kill_app()
    log = cmd_file.with_suffix(cmd_file.suffix + ".log")
    return lines, (log.read_text() if log.exists() else "")


def main():
    if len(sys.argv) != 3:
        die("用法:run-flow.py <考卷.toml> <run-dir>")
    flow_path, run_dir = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    if not APP.exists():
        die(f"未找到 {APP} —— 先跑 ./scripts/bundle-desktop.sh")
    meta, phases = parse_flow(flow_path)

    flow_dir = run_dir / "flows" / flow_path.stem
    (flow_dir / "snaps").mkdir(parents=True, exist_ok=True)
    shutil.copy(flow_path, flow_dir / "flow.toml")

    db_path = prep_db(meta["db"], run_dir)
    print(f"[run-flow] {flow_path.stem}: db={db_path}")

    all_cmds, all_logs = [], []
    for i, ph in enumerate(phases, 1):
        label = ph.get("name") or "main"
        print(f"[run-flow]   阶段 {i}/{len(phases)} · {label}")
        cmds, log = run_phase(ph, db_path, flow_dir, i)
        if len(phases) > 1:
            all_cmds.append(f"# ── 阶段 {label} ──")
            all_logs.append(f"# ── 阶段 {label} ──")
        all_cmds.extend(cmds)
        all_logs.append(log.rstrip())
    (flow_dir / "cmds").write_text("\n".join(all_cmds) + "\n")
    (flow_dir / "cmds.log").write_text("\n".join(x for x in all_logs if x) + "\n")

    verifies = [v for ph in phases for v in ph["verifies"]]
    with (flow_dir / "readback.jsonl").open("w") as f:
        for v in verifies:
            sql = v.get("sql")
            if not sql:
                die("考卷里有缺 sql 的 verify")
            out = subprocess.run(
                ["sqlite3", db_path, sql], capture_output=True, text=True
            )
            raw = out.stdout.strip() or f"(sqlite3 无输出/报错) {out.stderr.strip()}"
            f.write(json.dumps({"sql": sql, "raw": raw}, ensure_ascii=False) + "\n")
    print(f"[run-flow] {flow_path.stem}: 完成 → {flow_dir}")


if __name__ == "__main__":
    main()
