# TASKS · aihot 日报(构建阶段)

来源:`docs/SPEC.md` §8 验收标准表。AC-1~AC-14 已实现且测试覆盖(`tests/test_filter.py`
/`test_dedup.py`/`test_render.py`/`test_main.py` 现状),不生成任务。以下是 SPEC 明确
标注为「缺口」的 AC-15~AC-21,按 `spec-to-tests` 技能逐条翻译为任务,顺序=建议提交顺序
(独立的先做,共享 mock 基础设施的三个 main() 测试放最后依次复用)。每条任务粒度=一次
提交(一个新测试 + 使其通过所需的最小改动,若源码已满足行为则只补测试)。

| # | 任务 | 对应 AC | 落点 | 状态 |
|---|---|---|---|---|
| T1 | 补 `test_main_exits_2_on_config_error`:`--config` 指向不存在路径时 `main()` 返回码为 2 | AC-15 | `tests/test_main.py` | ✅ |
| T2 | 新建 `tests/test_arxiv.py`,补 `test_arxiv_items_have_honest_zero_score`:mock `urlopen` 返回固定 Atom XML,断言每条 `score == 0` | AC-21 | `tests/test_arxiv.py`(新) | ✅ |
| T3 | 新建 `tests/test_hackernews.py`,补 `test_single_item_fetch_failure_skips_item_not_whole_source`:mock 单个 item 拉取抛异常,断言该条被跳过、其余条目正常返回 | AC-20 | `tests/test_hackernews.py`(新) | ✅ |
| T4 | 在 `tests/test_hackernews.py` 补 `test_single_source_unreachable_other_source_still_succeeds`:mock 某个 story_list 拉取失败,断言 `fetch()` 记 0 条并继续处理其余 list,不抛异常 | AC-19 | `tests/test_hackernews.py` | ✅ |
| T5 | 在 `tests/test_main.py` 引入可复用的 mock fetch helper(patch `aihot.sources.hackernews.fetch`/`aihot.sources.arxiv.fetch`),补 `test_main_exits_1_and_writes_nothing_on_zero_hits`:两个源均 mock 为 0 命中,断言返回码 1 且 `--out` 目录下未写任何文件 | AC-16 | `tests/test_main.py` | ✅ |
| T6 | 复用 T5 的 mock helper,补 `test_main_exits_0_and_writes_digest_on_hits`:mock 命中数据,断言返回码 0 且 `<date>.md`/`.html`/`index.html` 三个文件真实存在 | AC-17 | `tests/test_main.py` | ✅ |
| T7 | 复用 T5 的 mock helper,补 `test_date_override_controls_output_filename`:传入 `--date` 覆盖值(非今天),断言输出文件名与日报标题使用覆盖值而非系统当前日期 | AC-18 | `tests/test_main.py` | ✅ |

## 完成定义(DoD)

- [x] 每个任务对应的测试新增后,`python3 -m unittest discover -s tests` 全绿(含既有 23 个用例,现共 30 个)。
- [x] `docs/SPEC.md` §8 表中对应 AC 的「现状」列由 ⬜ 更新为 ✅,并写明测试文件路径。
- [x] 不在本清单外新增功能范围(反模式:边建边改方向)——T1~T7 均只新增测试,未改动 `aihot/` 下任何生产代码(逐条核实过既有行为已正确)。
