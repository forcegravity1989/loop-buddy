# 健康检查真实执行记录 · 2026-07-20

`scripts/healthcheck.sh` 真实执行一遍,完整输出原样摘录(非美化):

```
== 单测 ==
test_empty_input (test_dedup.TestDedupe) ... ok
test_first_occurrence_wins (test_dedup.TestDedupe) ... ok
test_near_duplicate_titles_collapse_to_one (test_dedup.TestDedupe) ... ok
test_case_and_punctuation_insensitive (test_dedup.TestNormalizeTitle) ... ok
test_stopwords_dropped (test_dedup.TestNormalizeTitle) ... ok
test_caps_each_source_independently (test_filter.TestCapPerSource) ... ok
test_preserves_input_order_within_source (test_filter.TestCapPerSource) ... ok
test_zero_or_none_means_uncapped (test_filter.TestCapPerSource) ... ok
test_empty_keywords_excludes_everything (test_filter.TestFilterAndScore) ... ok
test_sorted_by_score_descending (test_filter.TestFilterAndScore) ... ok
test_zero_score_excluded_no_exceptions (test_filter.TestFilterAndScore) ... ok
test_case_insensitive_match (test_filter.TestScore) ... ok
test_no_match_is_zero (test_filter.TestScore) ... ok
test_searches_summary_too (test_filter.TestScore) ... ok
test_html_escapes_filenames (test_render.TestRender) ... ok
test_index_links_to_html_not_md (test_render.TestRender) ... ok
test_index_lists_only_dated_digest_files_newest_first (test_render.TestRender) ... ok
test_render_html_escapes_item_title (test_render.TestRender) ... ok
test_write_digest_creates_html_and_md_and_index (test_render.TestRender) ... ok

----------------------------------------------------------------------
Ran 19 tests in 0.004s

OK
== 冒烟运行(真实网络,生成今日日报)==
[main] 拉取 Hacker News(['topstories'])…
[main] 拉取 arXiv(['cs.AI', 'cs.CL', 'cs.LG'])…
[main] 原始条目:295(HN=198 arXiv=97)
[main] 命中=87 去重后=87 按源限量后=30
[main] 日报已生成:.../digests/2026-07-20.html(30 条)
== digests/ 真实产物核对 ==
== 全部通过 ==
```

退出码 0。三个产物文件(当日 .md/.html + index.html)真实存在性核对通过。
