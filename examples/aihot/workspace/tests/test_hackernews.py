import unittest
from unittest.mock import patch
from urllib.error import URLError

from aihot.sources import hackernews


def _item(item_id, title):
    return {
        "id": item_id,
        "type": "story",
        "title": title,
        "url": f"https://example.com/{item_id}",
        "score": 10,
        "time": 1700000000,
    }


class TestSingleItemFetchFailure(unittest.TestCase):
    def test_single_item_fetch_failure_skips_item_not_whole_source(self):
        def fake_get_json(url):
            if url == f"{hackernews.BASE}/topstories.json":
                return [1, 2, 3]
            if url == f"{hackernews.BASE}/item/2.json":
                raise URLError("boom")
            item_id = int(url.rsplit("/", 1)[-1].split(".")[0])
            return _item(item_id, f"Story {item_id}")

        with patch("aihot.sources.hackernews._get_json", side_effect=fake_get_json):
            items = hackernews.fetch(story_lists=["topstories"], fetch_limit=200)

        ids = {it["id"] for it in items}
        self.assertEqual(ids, {"1", "3"})
        self.assertNotIn("2", ids)


class TestSingleSourceListUnreachable(unittest.TestCase):
    def test_single_source_unreachable_other_source_still_succeeds(self):
        def fake_get_json(url):
            if url == f"{hackernews.BASE}/topstories.json":
                raise URLError("dns failure")
            if url == f"{hackernews.BASE}/newstories.json":
                return [5]
            if url == f"{hackernews.BASE}/item/5.json":
                return _item(5, "Still Works")
            raise AssertionError(f"unexpected url {url}")

        with patch("aihot.sources.hackernews._get_json", side_effect=fake_get_json):
            items = hackernews.fetch(story_lists=["topstories", "newstories"], fetch_limit=200)

        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]["id"], "5")


if __name__ == "__main__":
    unittest.main()
