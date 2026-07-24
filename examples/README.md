# examples/

真实、可加载的样板间(show-flat)数据——跟 `crates/bw-app/examples/*.rs`(可运行的
Rust 指挥器/示例程序)是两回事:这里放的是**数据**,不是代码。

目前只有一份:**`aihot/`**——2026-07-20 起真实践行「aihot 日报」项目留下的完整
SQLite 数据库(`bw-aihot.db`),不是为了给"以后随便什么 DB 都往这儿传"开口子。

## 加载(真的能打开,不是截图)

```bash
BW_DB=examples/aihot/bw-aihot.db cargo run -p app-desktop
# 或深链直达某个面板(渲染证明见 stderr [BW_OPEN]):
BW_DB=examples/aihot/bw-aihot.db BW_OPEN="aihot 日报" BW_PANEL=issues \
    cargo run -p app-desktop
```

会看到一个走完真实五阶段一整圈(+ 运维回流原型的二圈)的项目:31 个 Issue
(29 Done/1 InProgress/1 Todo)、1 个项目自有 agent、2 条项目自有 skill(1 条
真实蒸馏自 Issue)、真实健康信号——全部零 mock,是真跑出来的,不是摆拍数据。

## 这份 DB 之外还有什么(互补,不重复)

- [`iterations/PRACTICE-AIHOT.md`](../iterations/PRACTICE-AIHOT.md)——完整叙事:
  假设→动作→真实输出→结论,逐轮记录。
- [`iterations/AIHOT-EVIDENCE.json`](../iterations/AIHOT-EVIDENCE.json)——数字侧
  证据快照(不用打开 DB 也能读的纯文本版本),由 `crates/bw-app/examples/
  archive_aihot_evidence.rs` 从**这份 DB**(以及真实项目工作区的 git 证据)
  重新读回生成,两者对得上是这份归档可信的证明。

## 这份 DB 里没有的东西

真实项目工作区(`aihot/main.py` 等实现代码 + 其自己的真实 git 提交历史)**没有**
一并放进来——那是一个带独立 `.git` 的真实仓库,直接塞进这个仓库会造成嵌套
`.git` 的 gitlink 混乱。所以 `BW_DB` 能打开项目、看 Issue/agent/skill/信号,
但项目的 `workspace_path` 指向一个这份仓库里不存在的路径——任何真要读工作区
文件的功能(真实跑 agent、`SyncConnector` 拉真实 git 证据)在这个副本上会诚实
报"工作区不存在",不是静默假装。
