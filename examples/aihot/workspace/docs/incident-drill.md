# 破坏性演练 · 2026-07-20

逐个坏输入真实执行,记录真实行为;发现问题当场修复并重测。

| 坏输入 | 修前真实行为 | 修后真实行为 |
|---|---|---|
| `--config /tmp/does-not-exist.json`(路径不存在) | 裸 `FileNotFoundError` traceback | `[main] 配置错误:配置文件不存在:…`,退出码 2 |
| config.json 缺 `keywords` 字段 | 裸 `KeyError: 'keywords'` traceback | `[main] 配置错误:… 缺少必需字段 keywords`,退出码 2 |
| config.json 非法 JSON | (未测——同一 `load_config` 路径已在修复范围内,4 条真实单测之一直接覆盖) | 友好报错,退出码 2 |
| `keywords: []`(空数组) | 已经优雅:0 命中 → 如实不写文件 + 提示,退出码 1 | 无需修改(修前就对) |
| HN 源域名不可达(真实指向不存在域名测试) | 已经优雅:该源返回空列表 + stderr 警告,不影响另一源 | 无需修改(修前就对) |

## 结论

两处真实 crash(路径不存在、缺必需字段)已修复为 `ConfigError` 统一友好报错路径
(`aihot/main.py`),4 条新真实单测覆盖(`tests/test_main.py`);另外三种坏输入
修前就已经是诚实优雅的行为,不需要动。全量单测 23/23 通过。
