# examples/

真实、可加载的样板间(show-flat)数据——跟 `crates/bw-app/examples/*.rs`(可运行的
Rust 指挥器/示例程序)是两回事:这里放的是**数据**,不是代码。

**自足是这个目录的硬要求**:clone 下来就该可用,不依赖任何人本机的
`~/.claude/plugins/cache`、`~/Library/Application Support/` 或某个 worktree 里的
临时目录。这条规矩是踩出来的——第一版只提交了 DB,而 DB 里的 `workspace_path`
指向一个被 `.gitignore` 挡在仓外的目录,技能库又依赖本机插件缓存;结果多人协作时
对方 clone 完什么都打不开。

```
examples/
├── aihot/
│   └── bw-aihot.db         样板间 DB(裁剪自真实日常库)
└── skill-libraries/        vendor 进仓的官方技能库(MIT,见其 README)
    ├── mattpocock-skills/  41 条
    └── superpowers/        14 条
```

样板间项目的**代码仓不在这里**,它是一个独立的公开仓:
**https://github.com/forcegravity1989/aihot**(44 个文件,完整 32 次真实提交历史)。
DB 里记的是 `github_remote = forcegravity1989/aihot`,不是一份会随时间变旧的副本——
单一事实源,而且这才是产品自己的形态:plan/13「GitHub 主体化」整套
(issue=GitHub issue、验收=merge)都建在这个字段上。

## 加载(真的能打开,不是截图)

```bash
BW_DB=examples/aihot/bw-aihot.db cargo run -p app-desktop
```

深链直达某个面板(stderr 打 `[BW_OPEN]` 即渲染成功):

```bash
BW_DB=examples/aihot/bw-aihot.db BW_OPEN="aihot 日报" BW_PANEL=issues cargo run -p app-desktop
```

打开会看到:31 个 Issue(29 Done/1 InProgress/1 Todo)、65 条技能(mattpocock 41 +
superpowers 14 + bw-standard 3 + 自建 7,**全部带真正文**)、73 个智能体、
真实健康信号——零 mock,是真跑出来的,不是摆拍。

项目的 `workspace_path` **是空的**,状态诚实地是「有仓、还没克隆到本地」。想让
本地真有那份代码,用应用里的「克隆仓库」动作(走 `bw_engine::github::clone_repo`),
或者自己来一发:

```bash
git clone https://github.com/forcegravity1989/aihot
```

克隆完再在项目里设一下工作目录,版本面板就能读到那 32 次真实提交。

## 它是怎么来的(可复现,不是手工捏的)

`bw-aihot.db` 是**真实日常库的一份裁剪副本**,由
[`crates/bw-app/examples/build_aihot_fixture.rs`](../crates/bw-app/examples/build_aihot_fixture.rs)
生成——那把裁剪刀留在仓里,就是为了让"样板间怎么来的"可复核、可重跑:

```bash
cargo run -p bw-app --example build_aihot_fixture
```

它走真实 `Command` 层做四件事:源库只读复制 → 除 aihot 外的项目全部
`DeleteProject`(隐私边界,日常库里的其它项目不进公开仓)→ 清掉只在原作者机器上
成立的 `workspace_path`、记上真实公开仓 `github_remote` → 用仓内 vendor 的技能库
各导一次(正常结果 `imported=0 / skipped=全部`,这恰好证明仓内副本与日常库同源)。
最后 `VACUUM`——SQLite 删行只是标记空闲页,公开发布的文件里被删项目的原始字节
必须真的消失。

## 诚实的局限

- **打开一次就会写回一次**。`Boot`/`OpenProject` 会触发 `recompute_signals` 重算
  并落库,所以本地跑过样板间之后 `git status` 会显示 `bw-aihot.db` 有改动——
  内容没坏,是刷新了派生缓存与时间戳。不想提交这个改动就
  `git checkout -- examples/aihot/bw-aihot.db`。
- **代码仓要单独克隆**。DB 里只有 `github_remote`,没夹带文件副本——换来的是
  真实历史与单一事实源,代价是多一步(要联网;公开仓无需授权,匿名可克隆)。
- **技能库是快照,不跟上游同步**。见
  [`skill-libraries/README.md`](skill-libraries/README.md)。

## 相关材料(互补,不重复)

- [`docs/archive/iterations/PRACTICE-AIHOT.md`](../docs/archive/iterations/PRACTICE-AIHOT.md)——完整叙事:
  假设→动作→真实输出→结论,逐轮记录。
- [`docs/archive/iterations/AIHOT-EVIDENCE.json`](../docs/archive/iterations/AIHOT-EVIDENCE.json)——数字侧
  证据快照(不打开 DB 也能读的纯文本版本)。
