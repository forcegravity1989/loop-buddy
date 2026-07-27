"""Hacker News source — real, unauthenticated API (no key needed).

https://github.com/HackerNews/API
"""

import json
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from urllib.error import URLError

BASE = "https://hacker-news.firebaseio.com/v0"
TIMEOUT = 10


def _get_json(url):
    with urllib.request.urlopen(url, timeout=TIMEOUT) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _fetch_item(item_id):
    try:
        item = _get_json(f"{BASE}/item/{item_id}.json")
    except (URLError, TimeoutError, ValueError):
        return None
    if not item or item.get("type") != "story" or item.get("dead") or item.get("deleted"):
        return None
    title = item.get("title", "").strip()
    if not title:
        return None
    return {
        "source": "hackernews",
        "id": str(item_id),
        "title": title,
        "url": item.get("url") or f"https://news.ycombinator.com/item?id={item_id}",
        "score": item.get("score", 0),
        "time": item.get("time", 0),
    }


def fetch(story_lists=("topstories",), fetch_limit=200):
    """Real fetch: pull story-id lists, then item details concurrently.

    Returns a list of dicts (see `_fetch_item`); network/parse failures on
    individual items are skipped, not fatal (an unreachable single item is
    not "the source is down").
    """
    ids = []
    for lst in story_lists:
        try:
            ids.extend(_get_json(f"{BASE}/{lst}.json")[:fetch_limit])
        except (URLError, TimeoutError, ValueError) as e:
            print(f"[hackernews] 拉取 {lst} 失败(如实跳过,不是全源失败):{e}")
    ids = list(dict.fromkeys(ids))  # de-dupe while preserving order

    items = []
    with ThreadPoolExecutor(max_workers=16) as pool:
        for result in pool.map(_fetch_item, ids):
            if result is not None:
                items.append(result)
    return items
