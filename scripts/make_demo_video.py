#!/usr/bin/env python3
"""Generate the WorkflowHub demo short-video frames (iter 25 video deliverable).

Renders ~18s of animation (1280x720, 15fps) telling the story:
  Scene 1 (0-3s):  title + the 5-role ring lighting up
  Scene 2 (3-8s):  the 4 arcs filling in with iteration counts
  Scene 3 (8-15s): the self-optimizing demo — success bar 29% → 57%, A/B Improved
  Scene 4 (15-18s): headline metrics + 线闭成环

Frames land in /tmp/wfh-frames/ for ffmpeg to encode. Pure Pillow — no numpy.
CJK via PingFang.ttc (present on every macOS).
"""
import math, os, shutil
from PIL import Image, ImageDraw, ImageFont

W, H, FPS, DUR = 1280, 720, 15, 18
NFRAMES = FPS * DUR
OUT = "/tmp/wfh-frames"

# Design tokens (match the HTML report / project design system).
PAPER = (239, 235, 226); PAPER2 = (244, 239, 228); INK = (46, 42, 34); INK2 = (107, 99, 87)
CLAY = (197, 101, 74); LINE = (216, 208, 192)
GREEN = (95, 115, 85); AMBER = (181, 134, 47); RED = (176, 80, 58); UNKNOWN = (154, 147, 133)
ARC = [(197, 101, 74), (204, 139, 60), (110, 140, 90), (79, 126, 134)]
STAGE_COL = [(197, 101, 74), (204, 139, 60), (110, 140, 90), (79, 126, 134), (126, 107, 138)]
STAGE_NM = ["原型·求真", "构建·求成", "优化·求简", "运营推广·求增", "运维·求稳"]

FONT_CJK = "/System/Library/Fonts/PingFang.ttc"
def font(sz): return ImageFont.truetype(FONT_CJK, sz)

def ease(t):  # ease-in-out cubic
    return 4*t*t*t if t < 0.5 else 1 - ((-2*t + 2) ** 3) / 2

def lerp(a, b, t): return a + (b - a) * t
def lerp_col(c1, c2, t): return tuple(int(lerp(c1[i], c2[i], t)) for i in range(3))

def fade(t, start, length):
    """0 before start, ramps to 1 over `length`, stays 1."""
    if t < start: return 0.0
    if t >= start + length: return 1.0
    return ease((t - start) / length)

def bg(draw):
    draw.rectangle([0, 0, W, H], fill=PAPER)

def center_text(draw, cy, text, fnt, fill):
    bbox = draw.textbbox((0, 0), text, font=fnt)
    draw.text(((W - (bbox[2]-bbox[0]))/2, cy - (bbox[3]-bbox[1])/2), text, font=fnt, fill=fill)

def draw_ring(draw, cx, cy, r, t_global):
    """5 stages around a ring; each lights up over scene 1."""
    n = 5
    for i in range(n):
        ang = -math.pi/2 + i * 2*math.pi/n
        x = cx + r * math.cos(ang); y = cy + r * math.sin(ang)
        on = fade(t_global, 0.3 + i*0.35, 0.4)
        col = lerp_col((210, 205, 195), STAGE_COL[i], on)
        rr = int(lerp(34, 46, on))
        draw.ellipse([x-rr, y-rr, x+rr, y+rr], fill=col, outline=LINE)
        if on > 0.5:
            lbl = font(15)
            bbox = draw.textbbox((0,0), STAGE_NM[i], font=lbl)
            draw.text((x - (bbox[2]-bbox[0])/2, y - 5), STAGE_NM[i], font=lbl, fill=(255,255,255))
    # ring arc + reflux arrow (appears after all stages on)
    reflux = fade(t_global, 2.4, 0.5)
    if reflux > 0:
        bbox = draw.textbbox((0,0), "↻ 线闭成环", font=font(20))
        draw.text((cx - (bbox[2]-bbox[0])/2, cy - 14), "↻ 线闭成环", font=font(20), fill=lerp_col(PAPER, CLAY, reflux))

def draw_arcs(draw, t_global):
    """4 arcs as a horizontal track filling in during scene 2."""
    arcs = [("Arc1 · 数据基座", "iter 1–5", 0),
            ("Arc2 · 优化智能", "iter 6–12", 0.6),
            ("Arc3 · 自改进闭环", "iter 13–20", 1.2),
            ("Arc4 · 演示与模板", "iter 21–25", 1.8)]
    y0, segw, gap = 250, 250, 18
    total = segw*4 + gap*3
    x0 = (W - total)/2
    center_text(draw, 200, "25 轮 · 四弧线", font(26), CLAY)
    for i, (name, rng, off) in enumerate(arcs):
        on = fade(t_global, 3.0 + off, 0.5)
        x = x0 + i*(segw+gap)
        h = int(lerp(0, 120, on))
        if h > 0:
            draw.rounded_rectangle([x, y0+120-h, x+segw, y0+120], radius=8, fill=ARC[i])
        draw.text((x+8, y0+128), name, font=font(17), fill=INK)
        draw.text((x+8, y0+152), rng, font=font(14), fill=INK2)
        if on >= 1:
            draw.text((x+8, y0+8), f"{int(on*100)}%", font=font(13), fill=INK2)

def draw_demo(draw, t_global):
    """Scene 3: the self-optimizing demo. Bar grows 29% → 57%, then A/B badge."""
    # map scene time 8..15 to progress 0..1
    p = max(0.0, min(1.0, (t_global - 8.0) / 5.0))
    center_text(draw, 110, "自驱优化闭环 · 一个失败工作流的修复", font(26), CLAY)
    center_text(draw, 148, "度量 → 建议 → 人工修复 → 再度量 → 改善已证", font(16), INK2)
    # before / after bars
    bx, by, bw, bh = 290, 320, 700, 56
    draw.rounded_rectangle([bx, by, bx+bw, by+bh], radius=10, fill=(230,226,215))
    before_rate = 0.29
    after_rate = lerp(0.29, 0.857, ease(p))  # the real demo: 29% → 85.7% on the after-week slice
    # before bar (static, red)
    draw.rounded_rectangle([bx, by, bx+int(bw*before_rate), by+bh], radius=10, fill=RED)
    draw.text((bx+10, by+16), "第1周 · 29% (Red)", font=font(18), fill=(255,255,255))
    # after bar (growing, amber→green)
    ay = by + 80
    draw.rounded_rectangle([bx, ay, bx+bw, ay+bh], radius=10, fill=(230,226,215))
    cur = lerp(before_rate, after_rate, ease(p))
    col = RED if cur < 0.5 else (AMBER if cur < 0.8 else GREEN)
    draw.rounded_rectangle([bx, ay, bx+int(bw*cur), ay+bh], radius=10, fill=col)
    draw.text((bx+10, ay+16), f"第2周 · {cur*100:.0f}% ({'Red' if cur<0.5 else 'Amber' if cur<0.8 else 'Green'})", font=font(18), fill=(255,255,255))
    # A/B Improved badge appears near end
    if p > 0.7:
        b = fade(t_global, 13.0, 0.8)
        badge_col = lerp_col(PAPER, GREEN, b)
        draw.rounded_rectangle([bx+bw-180, ay+bh+26, bx+bw, ay+bh+70], radius=12, fill=badge_col, outline=GREEN)
        center_text(draw, ay+bh+48, "A/B = Improved  ·  +57pp", font(18), INK)

def draw_metrics(draw, t_global):
    """Scene 4: headline metrics."""
    p = fade(t_global, 15.0, 0.6)
    center_text(draw, 180, "最终形态", font(28), CLAY)
    cards = [("25", "五角色环迭代"), ("87→124", "测试 · 0 失败"), ("14", "纯分析函数"), ("+57pp", "A/B 改善")]
    cw, gap = 250, 24
    x0 = (W - (cw*4+gap*3))/2
    for i, (big, lbl) in enumerate(cards):
        on = fade(t_global, 15.2 + i*0.18, 0.4)
        x = x0 + i*(cw+gap)
        col = lerp_col(PAPER2, (251,248,241), on)
        draw.rounded_rectangle([x, 250, x+cw, 380], radius=12, fill=col, outline=LINE)
        if on > 0.2:
            center_text_at(draw, x+cw/2, 295, big, font(34), CLAY)
            center_text_at(draw, x+cw/2, 345, lbl, font(15), INK2)
    if p > 0.5:
        center_text(draw, 450, "✓ 可创建 static workflow    ✓ 可经 schedule 自驱优化", font(19), GREEN)
        center_text(draw, 500, "目标的「贴近用户习惯与场景」在创建入口 + 自驱闭环双落地", font(15), INK2)

def center_text_at(draw, cx, cy, text, fnt, fill):
    bbox = draw.textbbox((0,0), text, font=fnt)
    draw.text((cx - (bbox[2]-bbox[0])/2, cy - (bbox[3]-bbox[1])/2), text, font=fnt, fill=fill)

def render_frame(idx):
    t = idx / FPS
    img = Image.new("RGB", (W, H), PAPER)
    draw = ImageDraw.Draw(img)
    # subtle top brand bar
    draw.rectangle([0,0,W,6], fill=CLAY)
    if t < 3.0:
        f = fade(t, 0.0, 0.4)
        center_text(draw, 120, "WorkflowHub", font(40), lerp_col(PAPER, INK, f))
        center_text(draw, 172, "25 轮五角色五阶段自举", font(22), lerp_col(PAPER, CLAY, f))
        draw_ring(draw, W/2, 430, 150, t)
    elif t < 8.0:
        draw_arcs(draw, t)
        draw_ring(draw, W/2, 600, 70, t)
    elif t < 15.0:
        draw_demo(draw, t)
    else:
        draw_metrics(draw, t)
    # footer timestamp
    draw.text((24, H-34), f"Builders' Workbench · 自驱优化演示  {int(t//60):02d}:{t%60:05.2f}", font=font(13), fill=INK2)
    img.save(os.path.join(OUT, f"frame_{idx:04d}.png"))

def main():
    if os.path.isdir(OUT): shutil.rmtree(OUT)
    os.makedirs(OUT)
    for i in range(NFRAMES):
        render_frame(i)
        if i % 30 == 0:
            print(f"  frame {i}/{NFRAMES}")
    print(f"done: {NFRAMES} frames in {OUT}")

if __name__ == "__main__":
    main()
