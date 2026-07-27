import unittest
from unittest.mock import patch

from aihot.sources import arxiv

ATOM_FIXTURE = b"""<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2607.00001v1</id>
    <title>A Study of LLM Agents</title>
    <summary>We study agentic LLMs in depth.</summary>
    <published>2026-07-20T00:00:00Z</published>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/2607.00002v1</id>
    <title>Diffusion Models for Everyone</title>
    <summary>A survey of diffusion models.</summary>
    <published>2026-07-20T00:00:00Z</published>
  </entry>
</feed>
"""


class _FakeResponse:
    def __init__(self, data):
        self._data = data

    def read(self):
        return self._data

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class TestArxivHonestScore(unittest.TestCase):
    def test_arxiv_items_have_honest_zero_score(self):
        with patch("aihot.sources.arxiv.urllib.request.urlopen", return_value=_FakeResponse(ATOM_FIXTURE)):
            items = arxiv.fetch(categories=["cs.AI"], max_results=40)
        self.assertEqual(len(items), 2)
        for it in items:
            self.assertEqual(it["score"], 0)


if __name__ == "__main__":
    unittest.main()
