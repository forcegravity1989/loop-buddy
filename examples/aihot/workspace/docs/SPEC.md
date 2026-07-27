# SPEC · aihot 日报

规格来源:已完成一圈原型→构建→优化→运营推广→运维(`docs/retro.md`)+ 两条真实核验
过的假设(`docs/retro.md` 「假设 1/2 的真实核验结果」,round #19/#20)。本文档把这些
已验证的行为**固化成可测试的契约**,供后续构建阶段回归核对;不引入未验证的新范围。

真实起点(2026-07-20 健康检查,`docs/healthcheck-run.md`):23 个单测全绿,冒烟运行
真实生成 30 条命中的日报。本 SPEC 以此为准绳,不重新发明。

## 1. CLI 接口

```
python3 -m aihot.main [--config PATH] [--out DIR] [--date YYYY-MM-DD]
```

| 参数 | 默认值 | 说明 |
|---|---|---|
| `--config` | `config.json`(仓库根) | 配置文件路径 |
| `--out` | `digests/`(仓库根) | 日报输出目录 |
| `--date` | 今天真实日期(`datetime.date.today()`) | 覆盖日期,仅用于测试/补跑历史日期;不影响抓取内容,只影响输出文件名与日报标题 |

退出码:`0` 成功生成、`1` 无命中(诚实不写文件)、`2` 配置错误。

## 2. 输入契约:`config.json`

| 字段 | 类型 | 必需 | 默认 | 说明 |
|---|---|---|---|---|
| `keywords` | `string[]` | 是 | — | 关注面关键词;缺失或非数组 → `ConfigError` |
| `min_score` | `int` | 否 | `1` | 命中门槛(不含),≥ 此分才上日报 |
| `max_items_per_source` | `int \| null` | 否 | `null`(不裁剪) | 单个来源最多保留条数;`0`/`null` 视为不裁剪 |
| `hackernews.story_lists` | `string[]` | 否 | `["topstories"]` | HN 列表名(如 `topstories`) |
| `hackernews.fetch_limit` | `int` | 否 | `200` | 每个列表拉取的故事 ID 数上限(假设 2 核验:100 会让 HN 部分少填 47%,**保持 200**,见 `docs/retro.md`) |
| `arxiv.categories` | `string[]` | 否 | `["cs.AI"]` | arXiv 分类(如 `cs.AI`/`cs.CL`/`cs.LG`) |
| `arxiv.max_results` | `int` | 否 | `40` | 每个分类拉取的条目上限 |

打分匹配规则(假设 1 核验后维持不变,见 `docs/retro.md`):关键词对标题+摘要做
**子串、大小写不敏感**匹配(非词边界匹配)——`LLM`→`LLMs`、`agent`→`Agentic` 等
词形变化是设计意图内的命中,不是 bug。

## 3. 数据源契约

| 来源 | 协议 | 鉴权 | 单条目字段 |
|---|---|---|---|
| Hacker News | `hacker-news.firebaseio.com` REST | 无需 | `source`(`"hackernews"`)、`id`、`title`、`url`、`score`、`time` |
| arXiv | `export.arxiv.org` Atom XML,**必须用 `https`**(http 301 跳转,已知坑) | 无需 | `source`(`"arxiv"`)、`id`、`title`、`url`、`summary`、`score`(恒为 `0`,arXiv 无热度分,如实标 0 不编造)、`time` |

单条目抓取失败(网络错误/JSON 解析失败/非法条目)→ 跳过该条,不中断整体抓取。
单个来源列表整体不可达 → 该来源记 0 条并向 stderr 打印警告,**另一来源继续正常出日报**
(不是全源失败)。

## 4. 处理管线(顺序不可颠倒)

```
fetch(HN + arXiv) → filter_and_score → dedupe → cap_per_source → render
```

1. **filter_and_score**:按 `min_score` 过滤,按 `match_score` 降序排序。
2. **dedupe**:按归一化标题(小写、去标点、去停用词)判重,**先出现者留**——因为
   输入已按分数排序,等价于"同一事件多来源命中时,分数最高的那条留下"。
3. **cap_per_source**:对去重后的列表**按来源分别裁剪**(不是整体截断)——避免
   量大的来源(如 HN)把量小但同样真实相关的来源(如 arXiv)全部挤出日报
   (「多源体量控制法」)。
4. **render**:生成 Markdown + HTML 双格式,并重建 `index.html` 汇总页。

## 5. 输出契约

成功(命中数 > 0)时,在 `--out` 目录下真实写入:

| 文件 | 内容 |
|---|---|
| `<date>.md` | Markdown 日报,按来源分组,含关注面关键词、命中数、每条的命中关键词与摘要片段(≤160 字符) |
| `<date>.html` | 真实排版 HTML(非 Markdown 换后缀),标题/URL/摘要均 HTML 转义 |
| `index.html` | 从磁盘真实扫描 `*.md` 文件重建(不是内存态拼接),仅列 `YYYY-MM-DD.md` 命名的文件,按日期降序,链到对应 `.html` |

stdout(成功):`[main] 日报已生成:<path>(<N> 条)`。

命中数为 0(含关键词表为空、当日全部来源不可达等情形)→ **不写任何文件**,
stderr 说明原因,退出码 `1`。诚实原则:不写空文件冒充有内容。

## 6. 错误处理

| 场景 | 行为 | 退出码 |
|---|---|---|
| `--config` 路径不存在 | `ConfigError`,stderr `[main] 配置错误:配置文件不存在:<path>` | 2 |
| 配置内容非合法 JSON | `ConfigError`,stderr 含解析错误信息 | 2 |
| 配置缺 `keywords` 字段(或非数组) | `ConfigError`,stderr `… 缺少必需字段 keywords` | 2 |
| `keywords: []`(空数组) | 不报错,过滤后 0 命中 → 走「命中数为 0」路径 | 1 |
| 单来源域名/网络不可达 | 该来源记 0 条,stderr 警告,不中断另一来源 | 视最终命中数而定(0 或 1) |
| 全部来源不可达 | 命中数为 0 → 「命中数为 0」路径 | 1 |

## 7. 边界情况

- **HN/arXiv 条目标题重复**(同一事件跨来源报道)→ `dedupe` 收敛为 1 条,保留分数
  更高(排序后先出现)的那条。
- **标题/摘要含 HTML 特殊字符**(如 `<script>`)→ 渲染到 HTML 时必须转义,不可
  注入执行。
- **`max_items_per_source` 为 `0` 或未设置**→ 视为不裁剪,全部保留。
- **某来源命中数不足 cap**→ 该来源不受影响,照常全部保留(cap 只影响超出部分)。
- **`digests/` 目录下存在非日期命名文件**(如 `notes.md`)→ `index.html` 重建时
  忽略,不列入。
- **`--date` 覆盖日期**→ 只影响输出文件名与日报标题文本,不影响真实抓取内容
  (抓取的是"此刻"的真实数据,不是该日期的历史数据)。
- **Python 版本 < 3.9**→ `render.py` 使用 `str.removesuffix`,3.8 及更早会真实
  报错;README 已标注 `≥3.9` 要求。

## 8. 验收标准(AC,均可翻译为测试用例)

| # | 验收标准 | 建议测试名 | 现状 |
|---|---|---|---|
| AC-1 | 关键词子串、大小写不敏感匹配,标题与摘要都参与匹配 | `test_case_insensitive_match` / `test_searches_summary_too` | ✅ 已覆盖(`tests/test_filter.py`) |
| AC-2 | 命中分数 < `min_score` 的条目被排除,无例外(含空关键词表=全排除) | `test_zero_score_excluded_no_exceptions` / `test_empty_keywords_excludes_everything` | ✅ 已覆盖 |
| AC-3 | 过滤后结果按 `match_score` 降序排序 | `test_sorted_by_score_descending` | ✅ 已覆盖 |
| AC-4 | 归一化标题去重,大小写/标点/停用词不影响判重 | `test_near_duplicate_titles_collapse_to_one` / `test_case_and_punctuation_insensitive` / `test_stopwords_dropped` | ✅ 已覆盖(`tests/test_dedup.py`) |
| AC-5 | 去重时先出现者(即分数更高者,因输入已排序)保留 | `test_first_occurrence_wins` | ✅ 已覆盖 |
| AC-6 | `cap_per_source` 按来源独立裁剪,不影响其他来源;不足 cap 的来源不受影响;保留来源内原顺序(即分数高者) | `test_caps_each_source_independently` / `test_preserves_input_order_within_source` | ✅ 已覆盖(`tests/test_filter.py`) |
| AC-7 | `max_items_per_source` 为 `0`/`None` 时不裁剪 | `test_zero_or_none_means_uncapped` | ✅ 已覆盖 |
| AC-8 | 渲染输出为真实排版 HTML(含 `<!doctype html>`),不是 Markdown 换后缀 | `test_write_digest_creates_html_and_md_and_index` | ✅ 已覆盖(`tests/test_render.py`) |
| AC-9 | 标题/摘要中的 HTML 特殊字符在渲染时被转义,不可注入 | `test_render_html_escapes_item_title` | ✅ 已覆盖 |
| AC-10 | `index.html` 只列 `YYYY-MM-DD.md` 命名文件,按日期降序,链到 `.html` | `test_index_links_to_html_not_md` / `test_index_lists_only_dated_digest_files_newest_first` | ✅ 已覆盖 |
| AC-11 | `--config` 路径不存在时抛 `ConfigError`,不是裸 traceback | `test_missing_file_raises_config_error_not_raw_exception` | ✅ 已覆盖(`tests/test_main.py`) |
| AC-12 | 配置非合法 JSON 时抛 `ConfigError` | `test_invalid_json_raises_config_error` | ✅ 已覆盖 |
| AC-13 | 配置缺 `keywords` 字段时抛 `ConfigError` | `test_missing_keywords_field_raises_config_error` | ✅ 已覆盖 |
| AC-14 | 合法配置正常加载 | `test_valid_config_loads` | ✅ 已覆盖 |
| AC-15 | `main()` 遇 `ConfigError` 时退出码为 `2`,不抛出未捕获异常 | `test_main_exits_2_on_config_error` | ✅ 已覆盖(`tests/test_main.py`) |
| AC-16 | 命中数为 0 时不写任何 `<date>.md`/`.html` 文件,退出码为 `1` | `test_main_exits_1_and_writes_nothing_on_zero_hits` | ✅ 已覆盖(`tests/test_main.py`) |
| AC-17 | 命中数 > 0 时,`main()` 退出码为 `0`,且 `<date>.md`/`.html`/`index.html` 三个文件真实存在 | `test_main_exits_0_and_writes_digest_on_hits` | ✅ 已覆盖(`tests/test_main.py`,mock `hackernews.fetch`/`arxiv.fetch` 离线跑) |
| AC-18 | `--date` 覆盖时,输出文件名与日报标题使用覆盖值,不使用系统当前日期 | `test_date_override_controls_output_filename` | ✅ 已覆盖(`tests/test_main.py`) |
| AC-19 | 单个来源列表拉取失败时,记 0 条并继续,不中断另一来源(整体不因单源失败而失败) | `test_single_source_unreachable_other_source_still_succeeds` | ✅ 已覆盖(`tests/test_hackernews.py`) |
| AC-20 | 单个条目(HN item / arXiv entry)解析失败时跳过该条,不中断整个来源的抓取 | `test_single_item_fetch_failure_skips_item_not_whole_source` | ✅ 已覆盖(`tests/test_hackernews.py`) |
| AC-21 | arXiv 条目 `score` 恒为 `0`(无热度信号时如实标 0,不编造分值) | `test_arxiv_items_have_honest_zero_score` | ✅ 已覆盖(`tests/test_arxiv.py`) |

**AC-15 ~ AC-21 构建阶段已按 `spec-to-tests` 技能逐条补齐自动化单测**(见
`docs/TASKS.md` T1~T7 记录);行为本身在补测试前就已正确(源码/运维文档已
固化),补测试均未改动 `aihot/` 下的生产代码,只新增测试文件/用例。
