"""CLI 入口:fetch(HN + arXiv) → filter_and_score → dedupe → render。

顺序说明:先打分排序,再去重——`dedupe` 保留"第一次出现"的那条,打分排序过的
输入意味着同一事件的多个来源里,分数最高的那条被保留(而不是随机哪条先来)。
"""

import argparse
import datetime
import json
import sys
from pathlib import Path

from aihot.dedup import dedupe
from aihot.filter import cap_per_source, filter_and_score
from aihot.render import consecutive_days, write_digest, write_telemetry
from aihot.sources import arxiv, hackernews

DEFAULT_CONFIG = Path(__file__).resolve().parent.parent / "config.json"
DEFAULT_DIGESTS_DIR = Path(__file__).resolve().parent.parent / "digests"


class ConfigError(Exception):
    """配置有问题(路径不存在/不是合法 JSON/缺必需字段)——友好报错,不是裸
    traceback(2026-07-20 破坏性演练 #17 真实发现原来会崩,当场修)。"""


def load_config(path):
    try:
        raw = Path(path).read_text(encoding="utf-8")
    except FileNotFoundError:
        raise ConfigError(f"配置文件不存在:{path}") from None
    try:
        cfg = json.loads(raw)
    except json.JSONDecodeError as e:
        raise ConfigError(f"{path} 不是合法 JSON:{e}") from None
    if "keywords" not in cfg or not isinstance(cfg["keywords"], list):
        raise ConfigError(f"{path} 缺少必需字段 keywords(字符串数组)")
    return cfg


def build_digest(cfg, date_str):
    hn_cfg = cfg.get("hackernews", {})
    ax_cfg = cfg.get("arxiv", {})

    print(f"[main] 拉取 Hacker News({hn_cfg.get('story_lists', ['topstories'])})…", file=sys.stderr)
    hn_items = hackernews.fetch(
        story_lists=hn_cfg.get("story_lists", ["topstories"]),
        fetch_limit=hn_cfg.get("fetch_limit", 200),
    )
    print(f"[main] 拉取 arXiv({ax_cfg.get('categories', ['cs.AI'])})…", file=sys.stderr)
    ax_items = arxiv.fetch(
        categories=ax_cfg.get("categories", ["cs.AI"]),
        max_results=ax_cfg.get("max_results", 40),
    )
    raw_count = len(hn_items) + len(ax_items)
    print(f"[main] 原始条目:{raw_count}(HN={len(hn_items)} arXiv={len(ax_items)})", file=sys.stderr)

    scored = filter_and_score(hn_items + ax_items, cfg["keywords"], cfg.get("min_score", 1))
    deduped = dedupe(scored)
    capped = cap_per_source(deduped, cfg.get("max_items_per_source"))
    print(
        f"[main] 命中={len(scored)} 去重后={len(deduped)} 按源限量后={len(capped)}",
        file=sys.stderr,
    )

    return capped, raw_count, len(scored), len(deduped)


def main(argv=None):
    parser = argparse.ArgumentParser(description="生成 AI 热点日报")
    parser.add_argument("--config", default=str(DEFAULT_CONFIG))
    parser.add_argument("--out", default=str(DEFAULT_DIGESTS_DIR))
    parser.add_argument("--date", default=None, help="覆盖日期(测试用),默认今天真实日期")
    args = parser.parse_args(argv)

    try:
        cfg = load_config(args.config)
    except ConfigError as e:
        print(f"[main] 配置错误:{e}", file=sys.stderr)
        return 2
    date_str = args.date or datetime.date.today().isoformat()

    items, raw_count, hit_count, deduped_count = build_digest(cfg, date_str)
    if not items:
        print(
            f"[main] 今天(0 条真实来源可达或 0 条命中关注面,原始条目={raw_count})没有生成日报——"
            "如实不写空文件冒充有内容",
            file=sys.stderr,
        )
        # 命中数为 0 也是一次真实运行——如实记进 telemetry(命中率=0%),不是
        # 悄悄留着上一次的旧快照冒充"今天也正常"。
        write_telemetry(
            {
                "date": date_str,
                "raw": raw_count,
                "hit": hit_count,
                "deduped": deduped_count,
                "items": 0,
                "days": consecutive_days(date_str, args.out),
            },
            args.out,
        )
        return 1

    out_path = write_digest(items, date_str, cfg["keywords"], args.out)
    write_telemetry(
        {
            "date": date_str,
            "raw": raw_count,
            "hit": hit_count,
            "deduped": deduped_count,
            "items": len(items),
            "days": consecutive_days(date_str, args.out),
        },
        args.out,
    )
    print(f"[main] 日报已生成:{out_path}({len(items)} 条)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
