# REVIEW · aihot 日报(评审合入 · CI 门禁)

评审人:构建师(自查)。评审对象:`docs/SPEC.md` §8 全部 21 条验收标准 + 真实门禁命令输出。

**门禁适配说明**:本 phase 模板给的门禁命令(`cargo fmt`/`cargo clippy`/`cargo test`)
针对的是 Rust 仓库(Builders' Workbench 本体);`practice-aihot` 是独立的零依赖 Python
项目(无 `Cargo.toml`,无 `.github/workflows`),没有 Rust 工具链可跑。已核实工作区内
真实存在的等价门禁并执行:`scripts/lint.sh`(stdlib `compileall` 语法门禁,项目自述"零
依赖是设计选择")+ `python3 -m unittest discover`。以下门禁结果均为本次真实执行,非
复述上一棒记录。

## 1. 门禁真实执行结果

### 1.1 语法门禁 `./scripts/lint.sh`

```
$ ./scripts/lint.sh
语法门禁通过
$ echo $?
0
```

### 1.2 单元测试 `python3 -m unittest discover -s tests -v`

```
Ran 30 tests in 0.009s

OK
```

全 30 用例逐条 PASS(见下方 AC 表逐条核对),退出码 0。完整 verbose 输出已在本轮终端
真实执行(30 条 `ok`,含三条 `main()` 端到端用例的 stdout/stderr 真实日志,如
`[main] 日报已生成:…(1 条)`、`[hackernews] 拉取 topstories 失败(如实跳过,不是全源
失败):<urlopen error dns failure>`)。

**环境**:`python3 --version` → `Python 3.9.6`(满足 SPEC §7 标注的 `≥3.9` 要求)。

**未跑项说明**:仓库内无 `Cargo.toml`/`.github/workflows`,`cargo fmt`/`cargo clippy`
在本项目不适用,如实标注不跑,不伪造通过。

## 2. AC 逐条核对(§8 表,21 条)

核对方法:对每条 AC,(a) 确认 SPEC 表格标注的测试名在对应文件中真实存在
(`grep -rl "def <name>("`),(b) 读测试源码确认断言内容与 AC 描述一致,(c) 确认该测试
在本次真实跑测中处于 PASS 集合。

| AC | 结论 | 依据 |
|---|---|---|
| AC-1 | 通过 | `test_case_insensitive_match`/`test_searches_summary_too` 存在于 `tests/test_filter.py::TestScore`,断言子串+大小写不敏感、标题与摘要都参与匹配;PASS |
| AC-2 | 通过 | `test_zero_score_excluded_no_exceptions`/`test_empty_keywords_excludes_everything` 存在于 `tests/test_filter.py::TestFilterAndScore`;PASS |
| AC-3 | 通过 | `test_sorted_by_score_descending` 存在于同文件;PASS |
| AC-4 | 通过 | `test_near_duplicate_titles_collapse_to_one`/`test_case_and_punctuation_insensitive`/`test_stopwords_dropped` 存在于 `tests/test_dedup.py`;PASS |
| AC-5 | 通过 | `test_first_occurrence_wins` 存在于 `tests/test_dedup.py`;PASS |
| AC-6 | 通过 | `test_caps_each_source_independently`/`test_preserves_input_order_within_source` 存在于 `tests/test_filter.py::TestCapPerSource`;PASS |
| AC-7 | 通过 | `test_zero_or_none_means_uncapped` 存在于同文件;PASS |
| AC-8 | 通过 | `test_write_digest_creates_html_and_md_and_index` 存在于 `tests/test_render.py`;PASS |
| AC-9 | 通过 | `test_render_html_escapes_item_title` 存在于同文件;PASS |
| AC-10 | 通过 | `test_index_links_to_html_not_md`/`test_index_lists_only_dated_digest_files_newest_first` 存在于同文件;PASS |
| AC-11 | 通过 | `test_missing_file_raises_config_error_not_raw_exception` 存在于 `tests/test_main.py::TestLoadConfig`;PASS |
| AC-12 | 通过 | `test_invalid_json_raises_config_error` 存在于同类;PASS |
| AC-13 | 通过 | `test_missing_keywords_field_raises_config_error` 存在于同类;PASS |
| AC-14 | 通过 | `test_valid_config_loads` 存在于同类;PASS |
| AC-15 | 通过 | `test_main_exits_2_on_config_error` 存在于 `tests/test_main.py::TestMainExitCodes`,读源码确认直接调用 `main()` 断言返回码 `2`;PASS |
| AC-16 | 通过 | `test_main_exits_1_and_writes_nothing_on_zero_hits`,读源码确认 mock 两源 0 命中后断言返回码 `1` 且 `--out` 目录未写文件;PASS |
| AC-17 | 通过 | `test_main_exits_0_and_writes_digest_on_hits`,读源码确认 mock 命中后断言返回码 `0` 且 `.md`/`.html`/`index.html` 三文件真实 `.exists()`;PASS |
| AC-18 | 通过 | `test_date_override_controls_output_filename`,读源码确认断言文件名含覆盖日期且正文 `assertNotIn` 系统当前日期;PASS |
| AC-19 | 通过 | `test_single_source_unreachable_other_source_still_succeeds` 存在于 `tests/test_hackernews.py`,mock `topstories` 抛 `URLError`、`newstories` 正常,断言仍返回该条目;PASS |
| AC-20 | 通过 | `test_single_item_fetch_failure_skips_item_not_whole_source`,mock 单个 item 抛 `URLError`,断言该 id 被排除、其余 id 仍在;PASS |
| AC-21 | 通过 | `test_arxiv_items_have_honest_zero_score` 存在于 `tests/test_arxiv.py`,mock `urlopen` 返回固定 Atom 双条目,断言每条 `score == 0`;PASS |

**21/21 通过**。全部测试名均在声明文件中真实存在(非仅表格声称),断言内容与 AC 描述
一致,且全部处于本次真实 `unittest` 跑测的 PASS 集合内。

## 3. 交叉核实:`docs/TASKS.md`

T1~T7 状态列均为 ✅;DoD 三项(测试全绿现共 30 个 / SPEC 表现状列已更新 / 未在清单外
扩范围)逐项核对与本次真实跑测、SPEC 现状一致,未发现偏差。T1~T7 的 diff 范围经
`git diff --stat` 确认只涉及 `tests/test_main.py`(改)+ `tests/test_arxiv.py`/
`tests/test_hackernews.py`(增),`aihot/` 生产代码零改动,与 TASKS.md 「未改动生产
代码」的记录相符。

## 4. 结论

- 门禁:语法门禁通过、30/30 单测通过,均为本次真实执行,非复述。
- 验收标准:21/21 AC 全部通过,测试名真实存在且断言与描述一致。
- 未发现需要退回构建阶段的问题;可合入。
