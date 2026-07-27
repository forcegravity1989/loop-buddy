"""渲染:打分排序后的条目 → Markdown 日报(可翻旧账的纯文本存档)+ 真实排版的
HTML 日报(2026-07-20 growth 实验 #14 新增——修复 #13 发现的真实摩擦:浏览器
打开 .md 只看到源码,不是网页)+ 静态 index.html 汇总页。
"""

import datetime
import html
import json
import re
from pathlib import Path

_SOURCE_LABELS = {"hackernews": "Hacker News · 社区热度", "arxiv": "arXiv · 学术前沿"}


def _grouped(items):
    by_source = {}
    for it in items:
        by_source.setdefault(it["source"], []).append(it)
    return by_source


def render_markdown(items, date_str, keywords):
    lines = [f"# AI 热点日报 · {date_str}", ""]
    lines.append(f"关注面关键词:{', '.join(keywords)}")
    lines.append("")
    lines.append(f"共 {len(items)} 条命中(按命中度排序)")
    lines.append("")
    for src, src_items in _grouped(items).items():
        lines.append(f"## {_SOURCE_LABELS.get(src, src)}")
        lines.append("")
        for it in src_items:
            kw = "、".join(it.get("matched_keywords", []))
            lines.append(f"- **[{it['title']}]({it['url']})** _(命中: {kw})_")
            if it.get("summary"):
                snippet = it["summary"][:160]
                lines.append(f"  > {snippet}{'…' if len(it['summary']) > 160 else ''}")
        lines.append("")
    return "\n".join(lines)


def render_html(items, date_str, keywords):
    sections = []
    for src, src_items in _grouped(items).items():
        cards = []
        for it in src_items:
            kw = html.escape("、".join(it.get("matched_keywords", [])))
            title = html.escape(it["title"])
            url = html.escape(it["url"])
            summary_html = ""
            if it.get("summary"):
                snippet = html.escape(it["summary"][:220])
                summary_html = f'<p class="summary">{snippet}{"…" if len(it["summary"]) > 220 else ""}</p>'
            cards.append(
                f'<li class="card"><a class="title" href="{url}" target="_blank" rel="noopener">{title}</a>'
                f'<span class="kw">命中: {kw}</span>{summary_html}</li>'
            )
        sections.append(
            f'<section><h2>{html.escape(_SOURCE_LABELS.get(src, src))}</h2><ul class="cards">{"".join(cards)}</ul></section>'
        )

    return f"""<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>AI 热点日报 · {html.escape(date_str)}</title>
<style>
  body {{ font-family: -apple-system, "PingFang SC", sans-serif; max-width: 720px; margin: 32px auto; padding: 0 16px; color: #1a1a1a; }}
  h1 {{ font-size: 1.5rem; margin-bottom: 4px; }}
  .meta {{ color: #666; font-size: 0.85rem; margin-bottom: 24px; }}
  h2 {{ font-size: 1.1rem; border-bottom: 2px solid #eee; padding-bottom: 6px; margin-top: 32px; }}
  ul.cards {{ list-style: none; padding: 0; }}
  .card {{ padding: 12px 0; border-bottom: 1px solid #f0f0f0; }}
  .title {{ font-weight: 600; color: #0645ad; text-decoration: none; }}
  .title:hover {{ text-decoration: underline; }}
  .kw {{ display: block; font-size: 0.78rem; color: #888; margin-top: 2px; }}
  .summary {{ font-size: 0.88rem; color: #444; margin: 6px 0 0; }}
  a.back {{ font-size: 0.85rem; }}
</style>
</head>
<body>
<a class="back" href="index.html">← 全部日报</a>
<h1>AI 热点日报 · {html.escape(date_str)}</h1>
<p class="meta">关注面:{html.escape(', '.join(keywords))} · 共 {len(items)} 条命中</p>
{"".join(sections)}
</body>
</html>
"""


def write_digest(items, date_str, keywords, digests_dir):
    digests_dir = Path(digests_dir)
    digests_dir.mkdir(parents=True, exist_ok=True)
    md = render_markdown(items, date_str, keywords)
    (digests_dir / f"{date_str}.md").write_text(md, encoding="utf-8")
    out_path = digests_dir / f"{date_str}.html"
    out_path.write_text(render_html(items, date_str, keywords), encoding="utf-8")
    rebuild_index(digests_dir)
    return out_path


_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}\.md$")


def rebuild_index(digests_dir):
    """静态 index.html:列出 digests_dir 下所有真实存在的日报,新到旧,链到
    真实排版的 .html 版本(不是未渲染的 .md)。从磁盘真实扫描,不是从内存里
    的一次运行结果拼——重跑/补跑历史日期都对。
    """
    digests_dir = Path(digests_dir)
    dates = sorted(
        (f.name.removesuffix(".md") for f in digests_dir.glob("*.md") if _DATE_RE.match(f.name)),
        reverse=True,
    )
    items_html = "\n".join(f'    <li><a href="{html.escape(d)}.html">{html.escape(d)}</a></li>' for d in dates)
    page = f"""<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<title>aihot 日报</title>
<style>
  body {{ font-family: -apple-system, sans-serif; max-width: 640px; margin: 40px auto; padding: 0 16px; }}
  h1 {{ font-size: 1.4rem; }}
  li {{ margin: 6px 0; }}
</style>
</head>
<body>
<h1>aihot 日报</h1>
<p>共 {len(dates)} 期真实生成的日报(本页由 render.py 每次生成日报后自动重建):</p>
<ul>
{items_html}
</ul>
</body>
</html>
"""
    (digests_dir / "index.html").write_text(page, encoding="utf-8")


def consecutive_days(date_str, digests_dir):
    """从 date_str 真实往前数,逐天检查 <date>.md 是否存在,断了就停——真实
    连续产出天数,不是历史文件总数(有缺口的那些天不计入连续)。"""
    digests_dir = Path(digests_dir)
    d = datetime.date.fromisoformat(date_str)
    streak = 0
    while (digests_dir / f"{d.isoformat()}.md").exists():
        streak += 1
        d -= datetime.timedelta(days=1)
    return streak


def write_telemetry(stats, digests_dir):
    """本次真实运行的快照(BW 侧引领/滞后指标真喂的来源,见 plan/10 K4):
    {date, raw, hit, deduped, items, days}。每次跑完覆盖写这一个文件——不追加、
    不建历史版本,它就是"最新一次真实运行"的快照,不是时序存档。
    """
    digests_dir = Path(digests_dir)
    digests_dir.mkdir(parents=True, exist_ok=True)
    path = digests_dir / "telemetry.json"
    path.write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return path
