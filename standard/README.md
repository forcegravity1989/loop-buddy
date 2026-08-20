# standard/ 是什么

<!-- 30 秒导读:这是 standard/ 目录本身的说明文件,给三种人看——复核规范
     设计的用户、下一步要把这套规范接进 buddy 代码的会话、以后要往这里提
     PR 的同事。现在作数:这里是 V4「规范铺底」用的模板正本,但目前只有
     文档骨架,buddy 代码侧读它、铺它的实现还没写。 -->

`standard/` 是 Builders' Workbench(buddy)接入一个项目时,往那个项目仓里
铺的那一整套骨架文件的**正本**。项目仓里看到的
`PROJECT.md`、`AGENTS.md`、`.bw/metrics.toml` 这些文件,内容都来自这里的
模板渲染——改这里的模板,就是在改 buddy 给每个新项目铺出来的起点长什么样。

这套规范一共分八个大类(见
[`docs/v4-prototype/design/03-standard-and-backfill.md`](../docs/v4-prototype/design/03-standard-and-backfill.md)
§2.7 的完整目录表),这个仓目前只落地了会被优先接入代码的那一部分——见下方
文件表与「还没做的部分」。

## 谁会读这些文件

- **人**:复核这套规范设计对不对的用户;评审「规范铺底」这次合并请求时,
  人看到的骨架内容就是这里模板渲染出来的样子。
- **代码**:这个目录会在编译时整个打进 buddy 二进制(用 `include_str!`),
  铺底命令运行时从二进制里取模板写进项目仓,不是运行时去读磁盘上的这个
  目录——**改这里任何一份文件的内容,就是在改 buddy 的产品行为**,不是
  改一份纯文档。
- **agent**:铺进项目仓之后,`AGENTS.md`/`CLAUDE.md` 会被每次开工的 agent
  会话读到;其余模板渲染出的文件(`PROJECT.md`、`.bw/*.toml` 等)是项目
  自己的正本文件,供人和 agent 日常打开查阅、按需要修改。

## 目前这里有什么

| 文件 | 一句话 |
|---|---|
| `VERSION` | 当前规范版本号,铺底与对账时读这个值 |
| `01-charter/PROJECT.md.tmpl` | 项目章程模板:想做什么、最像的对标、北极星、项目信息 |
| `02-agents/AGENTS.md.tmpl` | 给 agent 的工作约定模板:先读什么、活怎么做、指标怎么碰、禁止事项等 |
| `02-agents/CLAUDE.md.tmpl` | 一行导入文件,把 `AGENTS.md` 接进 Claude CLI 会读的路径 |
| `03-docs/plan/README.md` | 说明 `docs/plan/` 目录的用途、正本地位、`origin` 字段含义 |
| `03-docs/plan/WEEK.md.tmpl` | 每周一份的周计划模板:周目标、业务活、指标读数、本周运作、上周完成情况 |
| `03-docs/releases.md.tmpl` | 发版记录模板,空表头,铺底时直接写进项目 `docs/releases.md` |
| `03-docs/design/README.md` | 说明 `docs/design/` 目录的用途与写作约定(30 秒导读、过期加横幅不删除) |
| `04-metrics/metrics.toml.tmpl` | `.bw/metrics.toml` 骨架:北极星占位 + 滞后/引领指标写法示例(注释掉) |
| `05-issue-policy/issue-policy.toml.tmpl` | `.bw/issue-policy.toml` 骨架:开工工具声明、类别→工具→workflow 默认映射、评审/节律/看板规则 |
| `08-meta/standard.toml.tmpl` | `.bw/standard.toml` 骨架:记录这个项目铺的规范版本与启用/扩展的大类 |

## 还没做的部分(如实说明,不是遗漏)

规范第 6 类「默认件与鱼塘」——buddy 自建的三份运作技能(更新指标与周计划
/ 资产盘点 / 规范铺底)、业界 workflow 包(mattpocock-skills、superpowers)
这些以 `.claude/skills/**/SKILL.md` 形态存在的技能包——**目前不在这个目录
里**。它们体量大、格式和这里的 TOML/Markdown 模板不是一回事,规划里留给
后续切片(B/C 段)单独做,不在这一批文档铺底里。`standard/06-defaults/`
与 `standard/pond/`(未严选技能的鱼塘目录)本身也还没建,不要假装它们已
经存在。

铺底流程与对账细节见
[`docs/v4-prototype/design/03-standard-and-backfill.md`](../docs/v4-prototype/design/03-standard-and-backfill.md);
文件格式的完整样例见
[`docs/v4-prototype/design/02-data-and-files.md`](../docs/v4-prototype/design/02-data-and-files.md) §2.5。
