# standard/ 是什么

<!-- 30 秒导读:这是 standard/ 目录本身的说明文件,给三种人看——复核规范
     设计的用户、维护铺底代码的会话、以后要往这里提 PR 的同事。现在作数:
     这里是 V4「规范铺底」用的模板正本,代码侧已经在读它、铺它了
     (`crates/bw-v4/src/standard/`)。 -->

`standard/` 是 Builders' Workbench(buddy)接入一个项目时,往那个项目仓里
铺的那一整套骨架文件的**正本**。项目仓里看到的 `.bw/PROJECT.md`、
`.bw/metrics.toml`、仓根 `AGENTS.md` 这些文件,内容都来自这里的模板渲染——改
这里的模板,就是在改 buddy 给每个新项目铺出来的起点长什么样。

**落点分两类,别混**:

- **`.bw/` 下的是 buddy 的资产** —— 名片、指标、周计划、发版记录、规范清单。
  收在一个目录里,「bw 到底往我仓里放了什么」一眼可数,也不会和项目自己的
  `README.md` / `docs/` 打架。
- **仓根 `AGENTS.md` 与 `CLAUDE.md` 是项目自己的** —— 那是 Claude Code /
  Cursor / Codex 都在找的位置,塞进 `.bw/` 谁都读不到。buddy 只在它们
  **不存在**时搭一份初稿,已经有就一个字不动,补齐交给规范铺底第 2 步的
  agent 会话去谈。

规范版本因此从 4.0 跳到 **5.0** —— 落点变了,老版本铺出来的文件在新版本眼里
就是「不在册」,不做迁移。

这套规范一共分八个大类(见
[`docs/v4-prototype/design.md`](../docs/v4-prototype/design.md) 第 4 章「规范铺底」
§2.7 的完整目录表),这个仓目前只落地了会被优先接入代码的那一部分——见下方
文件表与「还没做的部分」。

## 谁会读这些文件

- **人**:复核这套规范设计对不对的用户;评审「规范铺底」这次合并请求时,
  人看到的骨架内容就是这里模板渲染出来的样子。
- **代码**:这个目录会在编译时整个打进 buddy 二进制(用 `include_str!`),
  铺底命令运行时从二进制里取模板写进项目仓,不是运行时去读磁盘上的这个
  目录——**改这里任何一份文件的内容,就是在改 buddy 的产品行为**,不是
  改一份纯文档。
- **agent**:铺进项目仓之后,仓根 `AGENTS.md`(经 `CLAUDE.md` 一行导入)会被
  每次开工的 agent 会话读到;其余模板渲染出的文件(`.bw/PROJECT.md`、
  `.bw/*.toml` 等)是项目
  自己的正本文件,供人和 agent 日常打开查阅、按需要修改。

## 目前这里有什么

| 文件 | 一句话 |
|---|---|
| `VERSION` | 当前规范版本号,铺底与对账时读这个值 |
| `01-charter/PROJECT.md.tmpl` | 项目章程模板:想做什么、最像的对标、北极星、项目信息 |
| `03-docs/plan/README.md` | 说明 `.bw/plan/` 目录的用途、正本地位、`origin` 字段含义 |
| `03-docs/plan/WEEK.md.tmpl` | 每周一份的周计划模板:周目标、业务活、指标读数、本周运作、上周完成情况 |
| `03-docs/releases.md.tmpl` | 发版记录模板,空表头,铺底时直接写进项目 `.bw/releases.md` |
| `03-docs/design/README.md` | 说明 `.bw/design/` 目录的用途与写作约定(30 秒导读、过期加横幅不删除) |
| `04-metrics/metrics.toml.tmpl` | `.bw/metrics.toml` 骨架:北极星占位 + 滞后/引领指标写法示例(注释掉) |
| `05-issue-policy/issue-policy.toml.tmpl` | `.bw/issue-policy.toml` 骨架:开工工具声明、类别→工具→workflow 默认映射、评审/节律/看板规则 |
| `08-meta/standard.toml.tmpl` | `.bw/standard.toml` 骨架:记录这个项目铺的规范版本与启用/扩展的大类 |

## 还没做的部分(如实说明,不是遗漏)

规范第 6 类「默认件与鱼塘」的**运作技能已经建好**,在
`standard/06-defaults/ops/`:更新指标与周计划、资产盘点(它带一个子技能
`project-handbook` —— 给纳管的项目写开发手册,**先问人愿不愿意**才写)。
规范铺底不起 agent,所以没有它的剧本。它们和九份方法论技能(正本 `docs/skills/`)一起编进二进制,
开工时摊在 **buddy 自己的资产目录**,**不复制进用户的仓** —— 用户的
`.gitignore` 怎么写不该由 buddy 决定,而每个项目复制一份 buddy 自带的
东西也没有道理。系统提示词里只给名字 + 一句话 + 路径,正文按需读(见
`crates/bw-v4/src/standard/skills.rs`)。

还没建的是**鱼塘**:业界 workflow 包(mattpocock-skills、superpowers 等)
的清单与导入,`standard/pond/` 目录本身还不存在,不要假装它已经在。

铺底流程与对账细节见
[`docs/v4-prototype/design.md`](../docs/v4-prototype/design.md) 第 4 章「规范铺底」;
文件格式的完整样例见
[`docs/v4-prototype/design.md`](../docs/v4-prototype/design.md) 第 3 章「数据与文件」。
