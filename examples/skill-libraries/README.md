# examples/skill-libraries/ — vendor 进仓的官方技能库

这里是三个**第三方技能库的原样副本**,不是本仓自己写的内容。vendor 进来只为一件事:
让任何人 clone 之后都能真导入这批技能,而不是只有装过对应插件的那台机器才行。

## 为什么要 vendor(它解决的真实问题)

在此之前,`crates/bw-app/examples/` 里的导入示例把路径写死成
`/Users/<某人>/.claude/plugins/cache/...`——那是某一台笔记本上的插件缓存目录。
别人 clone 仓库跑同一条命令,只会得到 `root_path 不是一个存在的目录`;
随仓发布的样板间 DB 里也就没有这批技能。**"样板间"变成了指向本机状态的指针,
而不是自足的产物**,多人协作时对方什么都拿不到。

现在导入示例与 `build_aihot_fixture` 都指向这里,路径按 `CARGO_MANIFEST_DIR`
解析,在任何机器、任何工作目录下都成立。

## 内容与出处(均为 MIT,原 LICENSE 原样保留)

| 目录 | 上游 | 版本 | 版权 | 技能数 |
|---|---|---|---|---|
| `mattpocock-skills/` | https://github.com/mattpocock/skills | 1.2.0 | Copyright (c) 2026 Matt Pocock | 41 |
| `superpowers/` | https://github.com/obra/superpowers | 6.1.1 | Copyright (c) 2025 Jesse Vincent | 14 |
| `mohit-pm-claude-skills/` | https://github.com/mohitagw15856/pm-claude-skills | main@`f508142` (2026-08-04) | Copyright (c) 2026 Mohit Aggarwal | 2(见下,**部分 vendor**) |

三个库都是 MIT 许可,允许再分发,条件是保留版权声明与许可全文——各自目录下的
`LICENSE` 就是上游原件,**不要删、不要改**。技能正文同样一字未改:这是副本,
不是 fork,本仓不对其内容做任何修改或再创作。

前两个库 vendor 了各自完整的 `skills/` 目录;`mohit-pm-claude-skills/` **如实是部分
vendor**——上游是一个数百个通用 PM/生活技能的大型库(与本仓"指标"关注点绝大多数
无关),这里只挑了 `metrics-framework`(北极星/指标树/AARRR/HEART/OKR 框架)与
`metric-tree-builder`(北极星驱动因子分解)两件,是 plan/19 拿来主义测评(同模盲
测业界最佳同源方案之一,8.75/10)明确选中、合入 `docs/skills/north-star-discovery`
判据的那两件——vendor 它们是为了留一份可独立核验的原文副本,不是把整个库都拿来用。

只复制了各自的 `skills/` 目录(mohit 库只复制这两个技能子目录)与 `LICENSE`;上游
的 `package.json`、`node_modules`、CI 配置、其余技能与配套工具(`bin/`/`cockpit/`/
`connectors/` 等)没有带进来——它们与"导入这两件技能"这件事无关。

## 怎么用

```bash
# 导入到一个新库(幂等:重跑第二遍应当 imported=0 / skipped=全部)
cargo run -p bw-app --example import_skill_library -- /tmp/try.db

# 单个技能包导入(默认拿 mattpocock 的 engineering/tdd 做样例)
cargo run -p bw-app --example import_skill_package -- /tmp/try2.db
```

## 诚实的局限

这是**某一时刻的快照**,不会自动跟上游同步。上游发新版后,这里仍是 1.2.0 / 6.1.1 /
`f508142`,直到有人手工重新 vendor 并更新上表的版本号。要的是"任何人 clone 就能复现
同一份数据",代价就是它会随时间落后于上游——这是有意的取舍,不是疏忽。`mohit-pm-
claude-skills/` 额外的局限:这是部分 vendor,上游若给这两件技能发布新版本,这里也
不会自动跟上——同样只能靠手工重新 vendor。
