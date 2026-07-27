import unittest

from aihot.dedup import dedupe, normalize_title


class TestNormalizeTitle(unittest.TestCase):
    def test_case_and_punctuation_insensitive(self):
        self.assertEqual(
            normalize_title("GPT-5.6: A New Era!"),
            normalize_title("gpt 5 6 a new era"),
        )

    def test_stopwords_dropped(self):
        self.assertEqual(normalize_title("The Rise of the Machines"), "rise machines")


class TestDedupe(unittest.TestCase):
    def test_near_duplicate_titles_collapse_to_one(self):
        items = [
            {"title": "OpenAI announces GPT-6"},
            {"title": "OpenAI Announces GPT 6!"},  # same event, different punctuation
            {"title": "A totally different story"},
        ]
        out = dedupe(items)
        self.assertEqual(len(out), 2)

    def test_first_occurrence_wins(self):
        items = [{"title": "Same Story", "id": "first"}, {"title": "same story", "id": "second"}]
        out = dedupe(items)
        self.assertEqual(len(out), 1)
        self.assertEqual(out[0]["id"], "first")

    def test_empty_input(self):
        self.assertEqual(dedupe([]), [])


if __name__ == "__main__":
    unittest.main()
