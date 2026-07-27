"""关键词关注面打分法 —— 见 .claude/standards/skill-standards.md 蒸馏的方法论。

命中数 = 分数;0 分不上日报,没有例外。
"""


def score(item, keywords):
    haystack = (item.get("title", "") + " " + item.get("summary", "")).lower()
    hits = [kw for kw in keywords if kw.lower() in haystack]
    return len(hits), hits


def filter_and_score(items, keywords, min_score=1):
    scored = []
    for item in items:
        s, hits = score(item, keywords)
        if s >= min_score:
            scored.append({**item, "match_score": s, "matched_keywords": hits})
    scored.sort(key=lambda i: i["match_score"], reverse=True)
    return scored


def cap_per_source(items, max_per_source):
    """按 source 分别限量,而不是对合并后的列表整体截断——否则量大的来源
    (如 HN)会把量小但同样真实相关的来源(如 arXiv)全部挤出日报之外。
    输入需已按分数排序;每个来源内保留分数最高的前 N 条。
    """
    if not max_per_source:
        return items
    counts = {}
    out = []
    for item in items:
        src = item.get("source", "")
        counts[src] = counts.get(src, 0) + 1
        if counts[src] <= max_per_source:
            out.append(item)
    return out
