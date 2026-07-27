import contextlib
import datetime
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from aihot.main import ConfigError, load_config, main


@contextlib.contextmanager
def mock_fetch(hn_items=(), ax_items=()):
    """复用的 mock fetch helper(T5):patch `aihot.sources.hackernews.fetch`
    /`aihot.sources.arxiv.fetch`,让 main() 端到端测试脱离真实网络。"""
    with patch("aihot.main.hackernews.fetch", return_value=list(hn_items)), patch(
        "aihot.main.arxiv.fetch", return_value=list(ax_items)
    ):
        yield


def _write_config(tmpdir, **overrides):
    cfg = {"keywords": ["LLM"], "min_score": 1}
    cfg.update(overrides)
    path = Path(tmpdir) / "config.json"
    path.write_text(json.dumps(cfg), encoding="utf-8")
    return str(path)


class TestLoadConfig(unittest.TestCase):
    def test_missing_file_raises_config_error_not_raw_exception(self):
        with self.assertRaises(ConfigError):
            load_config("/tmp/definitely-does-not-exist-aihot.json")

    def test_invalid_json_raises_config_error(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            f.write("{not valid json")
            path = f.name
        try:
            with self.assertRaises(ConfigError):
                load_config(path)
        finally:
            Path(path).unlink()

    def test_missing_keywords_field_raises_config_error(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"min_score": 1}, f)
            path = f.name
        try:
            with self.assertRaises(ConfigError):
                load_config(path)
        finally:
            Path(path).unlink()

    def test_valid_config_loads(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"keywords": ["LLM"]}, f)
            path = f.name
        try:
            cfg = load_config(path)
            self.assertEqual(cfg["keywords"], ["LLM"])
        finally:
            Path(path).unlink()


class TestMainExitCodes(unittest.TestCase):
    def test_main_exits_2_on_config_error(self):
        code = main(["--config", "/tmp/definitely-does-not-exist-aihot.json"])
        self.assertEqual(code, 2)

    def test_main_exits_1_and_writes_no_digest_on_zero_hits(self):
        # AC-16 是"不写日报文件"(.md/.html),不是"什么都不写"——2026-07-21
        # 加了 telemetry.json 真实快照后,0 命中也是一次真实运行,如实记
        # 命中率=0%,不是悄悄留着上一次的旧快照冒充"今天也正常"。
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = _write_config(tmpdir)
            out_dir = Path(tmpdir) / "digests"
            with mock_fetch(hn_items=[], ax_items=[]):
                code = main(["--config", config_path, "--out", str(out_dir), "--date", "2026-07-21"])
            self.assertEqual(code, 1)
            self.assertFalse((out_dir / "2026-07-21.md").exists())
            self.assertFalse((out_dir / "2026-07-21.html").exists())
            telemetry = json.loads((out_dir / "telemetry.json").read_text(encoding="utf-8"))
            self.assertEqual(telemetry["date"], "2026-07-21")
            self.assertEqual(telemetry["hit"], 0)
            self.assertEqual(telemetry["items"], 0)
            self.assertEqual(telemetry["days"], 0)

    def test_main_exits_0_and_writes_digest_on_hits(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = _write_config(tmpdir)
            out_dir = Path(tmpdir) / "digests"
            hn_items = [
                {
                    "source": "hackernews",
                    "id": "1",
                    "title": "New LLM breaks benchmarks",
                    "url": "https://example.com/1",
                    "score": 100,
                    "time": 1700000000,
                }
            ]
            with mock_fetch(hn_items=hn_items, ax_items=[]):
                code = main(["--config", config_path, "--out", str(out_dir), "--date", "2026-07-21"])
            self.assertEqual(code, 0)
            self.assertTrue((out_dir / "2026-07-21.md").exists())
            self.assertTrue((out_dir / "2026-07-21.html").exists())
            self.assertTrue((out_dir / "index.html").exists())

    def test_telemetry_json_matches_real_run_counts(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = _write_config(tmpdir)
            out_dir = Path(tmpdir) / "digests"
            hn_items = [
                {
                    "source": "hackernews",
                    "id": str(i),
                    "title": f"LLM story {i}" if i < 3 else f"unrelated story {i}",
                    "url": f"https://example.com/{i}",
                    "score": 100,
                    "time": 1700000000,
                }
                for i in range(5)
            ]
            with mock_fetch(hn_items=hn_items, ax_items=[]):
                code = main(["--config", config_path, "--out", str(out_dir), "--date", "2026-07-21"])
            self.assertEqual(code, 0)
            telemetry = json.loads((out_dir / "telemetry.json").read_text(encoding="utf-8"))
            self.assertEqual(telemetry["date"], "2026-07-21")
            self.assertEqual(telemetry["raw"], 5)
            self.assertEqual(telemetry["hit"], 3)  # 只有 3 条标题含 "LLM"
            self.assertEqual(telemetry["items"], 3)
            self.assertEqual(telemetry["days"], 1)  # 前一天(07-20)没有真实文件,不连续

    def test_telemetry_days_counts_real_consecutive_streak_not_total(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = _write_config(tmpdir)
            out_dir = Path(tmpdir) / "digests"
            hn_items = [
                {
                    "source": "hackernews",
                    "id": "1",
                    "title": "LLM streak test",
                    "url": "https://example.com/1",
                    "score": 100,
                    "time": 1700000000,
                }
            ]
            with mock_fetch(hn_items=hn_items, ax_items=[]):
                main(["--config", config_path, "--out", str(out_dir), "--date", "2026-07-19"])
                main(["--config", config_path, "--out", str(out_dir), "--date", "2026-07-20"])
                # 中间断一天(没有 07-21),07-22 应该只算 1 天连续,不是 3
                main(["--config", config_path, "--out", str(out_dir), "--date", "2026-07-22"])
            telemetry = json.loads((out_dir / "telemetry.json").read_text(encoding="utf-8"))
            self.assertEqual(telemetry["days"], 1)

    def test_date_override_controls_output_filename(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = _write_config(tmpdir)
            out_dir = Path(tmpdir) / "digests"
            hn_items = [
                {
                    "source": "hackernews",
                    "id": "1",
                    "title": "LLM overrides the calendar",
                    "url": "https://example.com/1",
                    "score": 100,
                    "time": 1700000000,
                }
            ]
            override_date = "2020-01-01"
            with mock_fetch(hn_items=hn_items, ax_items=[]):
                code = main(["--config", config_path, "--out", str(out_dir), "--date", override_date])
            self.assertEqual(code, 0)
            digest_path = out_dir / f"{override_date}.md"
            self.assertTrue(digest_path.exists())
            content = digest_path.read_text(encoding="utf-8")
            self.assertIn(override_date, content)
            self.assertNotIn(datetime.date.today().isoformat(), content)


if __name__ == "__main__":
    unittest.main()
