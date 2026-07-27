"""arXiv source — real, unauthenticated Atom API.

https://arxiv.org/help/api

Note: `export.arxiv.org` 301-redirects http→https (verified 2026-07-20
during 践行) — use https directly, don't rely on a redirect follow that a
future stdlib default might not grant.
"""

import urllib.request
import xml.etree.ElementTree as ET
from urllib.error import URLError
from urllib.parse import quote

BASE = "https://export.arxiv.org/api/query"
ATOM_NS = "{http://www.w3.org/2005/Atom}"
TIMEOUT = 15


def fetch(categories=("cs.AI",), max_results=40):
    """Real fetch: one query per category (arXiv's `cat:` OR-syntax is
    finicky across categories), merged and de-duped by arXiv id.
    """
    seen_ids = set()
    items = []
    for cat in categories:
        query = f"search_query=cat:{quote(cat)}&sortBy=submittedDate&sortOrder=descending&max_results={max_results}"
        url = f"{BASE}?{query}"
        try:
            with urllib.request.urlopen(url, timeout=TIMEOUT) as resp:
                xml_bytes = resp.read()
        except (URLError, TimeoutError) as e:
            print(f"[arxiv] 拉取 {cat} 失败(如实跳过,不是全源失败):{e}")
            continue

        root = ET.fromstring(xml_bytes)
        for entry in root.findall(f"{ATOM_NS}entry"):
            arxiv_id = (entry.findtext(f"{ATOM_NS}id") or "").strip()
            if not arxiv_id or arxiv_id in seen_ids:
                continue
            seen_ids.add(arxiv_id)
            title = " ".join((entry.findtext(f"{ATOM_NS}title") or "").split())
            summary = " ".join((entry.findtext(f"{ATOM_NS}summary") or "").split())
            published = entry.findtext(f"{ATOM_NS}published") or ""
            if not title:
                continue
            items.append(
                {
                    "source": "arxiv",
                    "id": arxiv_id,
                    "title": title,
                    "url": arxiv_id,
                    "summary": summary,
                    "score": 0,  # arXiv has no popularity score — honest 0, not invented
                    "time": published,
                }
            )
    return items
