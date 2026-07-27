#!/usr/bin/env python3
"""把一次验收跑的截图串成带字幕的视频帧(供 encode-mp4.swift 编码)。

    python3 scripts/make-flow-frames.py <run-dir>   → <run-dir>/video-frames/NNNN.png

**为什么要有视频**:HTML 报告是给人**核对**的(逐条对照 SQL 原值);视频是给人
**看懂**的——一眼看清"点了什么、界面怎么变、最后库里是什么"。两者同源:帧只
从 `snaps/` 的真实截图来,字幕只从驱动自产的 `cmds.log` 来,**绝不另造画面**。

不依赖 ffmpeg(本机没有);编码交给 macOS 自带的 AVFoundation(见同目录
encode-mp4.swift)。字体用系统 PingFang,中文不豆腐块。
"""

import json
import re
import shutil
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

W, H = 1440, 900
TOP_H, BOT_H = 46, 48
PAPER = (239, 235, 226)
INK = (43, 38, 34)
INK3 = (138, 133, 120)
CLAY = (197, 101, 74)
GREEN = (74, 124, 89)
RED = (176, 65, 62)
CJK = "/System/Library/Fonts/PingFang.ttc"
# 命令行与 SQL 本可以用等宽的 Menlo,但**它没有中文字形**——考卷里的命令和
# SQL 都带中文(Issue 标题进了 WHERE 子句),用 Menlo 会整片渲染成豆腐块 □。
# 字幕看不懂就不叫证据,所以这里一律用 PingFang:可读性优先于等宽美观。
MONO = CJK

LOG_RE = re.compile(r"^\[BW_FLOW\] (\d+) (.*?) (ok|fail: .*)$")
PHASE_RE = re.compile(r"^#\s*──\s*阶段 (.+?)\s*──")


def font(path, size):
    try:
        return ImageFont.truetype(path, size)
    except OSError:
        return ImageFont.load_default()


F_TITLE = font(CJK, 34)
F_SUB = font(CJK, 17)
F_BAR = font(CJK, 16)
F_MONO = font(MONO, 14)
F_SMALL = font(CJK, 13)


def fit(draw, text, fnt, max_w):
    """超宽就截断加省略号——字幕宁可短,不许压扁变形。"""
    if draw.textlength(text, font=fnt) <= max_w:
        return text
    while text and draw.textlength(text + "…", font=fnt) > max_w:
        text = text[:-1]
    return text + "…"


def card(lines, title=None, sub=None):
    """纯文字卡片帧(片头 / 流片头 / SQL 读回页)。"""
    img = Image.new("RGB", (W, H), PAPER)
    d = ImageDraw.Draw(img)
    y = 120
    if title:
        d.text((90, y), title, font=F_TITLE, fill=CLAY)
        y += 58
    if sub:
        d.text((90, y), fit(d, sub, F_SUB, W - 180), font=F_SUB, fill=INK3)
        y += 44
    y += 16
    for text, kind in lines:
        color = {"ok": GREEN, "fail": RED, "mono": INK, "dim": INK3}.get(kind, INK)
        fnt = F_MONO if kind == "mono" else F_SUB
        for chunk in wrap(d, text, fnt, W - 180):
            d.text((90, y), chunk, font=fnt, fill=color)
            y += 30
        y += 6
    return img


def wrap(draw, text, fnt, max_w):
    out, cur = [], ""
    for ch in text:
        if draw.textlength(cur + ch, font=fnt) > max_w:
            out.append(cur)
            cur = ch
        else:
            cur += ch
    if cur:
        out.append(cur)
    return out or [""]


def shot_frame(png, flow_name, phase, action, verdict, snap_name):
    """一张真实截图 + 上下字幕条。截图区严格 1:1 放置,不缩放不裁切。"""
    img = Image.new("RGB", (W, H), PAPER)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W, TOP_H], fill=(255, 253, 249))
    head = flow_name if not phase else f"{flow_name} · {phase}"
    d.text((22, 13), head, font=F_BAR, fill=INK)
    ok = verdict == "ok"
    tag = "ok" if ok else verdict
    tw = d.textlength(tag, font=F_BAR)
    d.rectangle([W - tw - 40, 11, W - 16, TOP_H - 11], fill=GREEN if ok else RED)
    d.text((W - tw - 28, 13), tag, font=F_BAR, fill=(255, 255, 255))

    shot = Image.open(png).convert("RGB")
    if shot.size != (W, H - TOP_H - BOT_H):
        shot = shot.resize((W, H - TOP_H - BOT_H))
    img.paste(shot, (0, TOP_H))

    d.rectangle([0, H - BOT_H, W, H], fill=(255, 253, 249))
    label = f"{action}" if action else f"snap {snap_name}"
    d.text((22, H - BOT_H + 15), fit(d, label, F_MONO, W - 260), font=F_MONO, fill=INK)
    d.text(
        (W - 210, H - BOT_H + 16),
        f"📸 {snap_name}",
        font=F_SMALL,
        fill=INK3,
    )
    return img


def read_flow_meta(flow_dir):
    """从考卷副本取 name/purpose(定向逐行提取,理由同 gen-flow-report.py)。"""
    meta = {}
    toml = flow_dir / "flow.toml"
    if toml.exists():
        for line in toml.read_text().splitlines():
            s = line.strip()
            for key in ("name", "purpose"):
                if s.startswith(f"{key} ") or s.startswith(f"{key}="):
                    v = s.split("=", 1)[1].strip()
                    if v.startswith('"') and v.endswith('"'):
                        meta[key] = v[1:-1]
    return meta


def build(run_dir):
    out = run_dir / "video-frames"
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    meta_json = json.loads((run_dir / "meta.json").read_text())
    frames = []

    frames.append(
        card(
            [
                ("五条常青流 · 真实点击 · 真实读回", "dim"),
                (f"批次 {meta_json.get('batch', '?')}", "mono"),
                (f"commit {meta_json.get('commit', '?')}", "mono"),
                (f"{meta_json.get('started_at', '?')}", "mono"),
            ],
            title="Builders' Workbench 验收回放",
            sub="每一帧都是应用真实渲染的截图,字幕取自驱动自产日志——无一处摆拍",
        )
    )

    for flow_dir in sorted((run_dir / "flows").iterdir()):
        if not flow_dir.is_dir():
            continue
        fm = read_flow_meta(flow_dir)
        fname = fm.get("name", flow_dir.name)
        frames.append(
            card([], title=flow_dir.name, sub=fm.get("purpose", ""))
        )

        log = flow_dir / "cmds.log"
        phase, last_action, last_verdict = None, None, "ok"
        for line in (log.read_text().splitlines() if log.exists() else []):
            ph = PHASE_RE.match(line.strip())
            if ph:
                phase = ph.group(1)
                continue
            m = LOG_RE.match(line.strip())
            if not m:
                continue
            cmd, verdict = m.group(2), m.group(3)
            if cmd.startswith("snap "):
                snap_name = cmd[5:].strip()
                png = flow_dir / "snaps" / f"{snap_name}.png"
                if png.exists():
                    frames.append(
                        shot_frame(
                            png, fname, phase, last_action, last_verdict, snap_name
                        )
                    )
                continue
            last_action, last_verdict = cmd, verdict

        rb = flow_dir / "readback.jsonl"
        if rb.exists() and rb.read_text().strip():
            lines = [("SQL 读回(报告里给原值,读者自行对照)", "dim")]
            for row in rb.read_text().splitlines():
                if not row.strip():
                    continue
                r = json.loads(row)
                lines.append((r["sql"], "mono"))
                lines.append((f"→ {r['raw']}", "ok"))
            frames.append(card(lines, title=f"{flow_dir.name} · 读回"))

    for i, im in enumerate(frames):
        im.save(out / f"{i:04d}.png")
    print(f"[frames] {len(frames)} 帧 → {out}")
    return len(frames)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("用法:make-flow-frames.py <run-dir>")
    build(Path(sys.argv[1]).resolve())
