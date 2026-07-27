"""去重 —— 按归一化标题(小写、去标点、去停用词)判等,不要求逐字相同。"""

import re

_STOPWORDS = {"a", "an", "the", "of", "for", "to", "in", "on", "with", "is", "are"}
_PUNCT_RE = re.compile(r"[^\w\s]")


def normalize_title(title):
    t = _PUNCT_RE.sub(" ", title.lower())
    words = [w for w in t.split() if w and w not in _STOPWORDS]
    return " ".join(words)


def dedupe(items):
    """First occurrence wins (input order); ties broken by whichever the
    caller sorted first — callers that want "highest score wins" should sort
    before calling this."""
    seen = set()
    out = []
    for item in items:
        key = normalize_title(item["title"])
        if key in seen or not key:
            continue
        seen.add(key)
        out.append(item)
    return out
