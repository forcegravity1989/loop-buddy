# buddy 资产目录

> **30 秒导读**:这里是 buddy 系统提示词与规范的权威正本,给所有在 buddy 纳管项目里干活的 AI 队友用。现在作数。随产品版本维护,可被后续 buddy 重构直接继承。

## 目录职责

- `system-prompt.md` — 公共系统提示词正本:身份、工作方式、所有 Issue 共用铁律、规范索引。所有 Issue 首次开工时由 buddy 注入 Claude 会话。
- `standards/metrics.md` — 项目指标文件 `.bw/metrics.toml` 的格式规范。修改指标前必读。
- `standards/connectors.md` — 脚本连接器文件 `.bw/connectors.toml` 的格式规范。搭采集装置前必读。

buddy 规范回答「在 buddy 纳管的项目里,什么必须做对」,与 `docs/skills/` 下的 Skill 资产(回答「某类活怎样做得更好」)是两个独立维度,互不依附。换一个 Skill 后仍必须成立的,是 buddy 规范;只描述「如何把某类活做好」的,是 Skill。

## 新增一份 buddy 规范

1. 在 `standards/` 下增加一份有 30 秒导读的 Markdown 正本;
2. 在 `system-prompt.md` 的规范索引里补上「什么事务必须读它」和运行时相对路径;
3. 由打包与启动自检核对:索引列出的文件都存在、目录里的规范都已进入索引。两边不一致就阻止发布或启动,不能让半套规范静默生效。

## 运行副本

Claude 在业务项目工作区运行时,buddy 会把 `standards/*.md` 物化为 `<工作区>/.claude/buddy/standards/` 下的运行副本(带 `.bw-managed` 标记,不进用户 Git 提交)。`system-prompt.md` 正文由 buddy 直接用于启动参数,不复制运行副本。详细规则见设计篇 `docs/v2-prototype/issue-dispatch-prompt-skill.md` §4.3。
