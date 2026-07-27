# examples/

真实、可加载的样板间(show-flat)数据——跟 `crates/bw-app/examples/*.rs`(可运行的
Rust 指挥器/示例程序)是两回事:这里放的是**数据**,不是代码。

**自足是这个目录的硬要求**:clone 下来就该完整可用,不依赖任何人本机的
`~/.claude/plugins/cache`、`~/Library/Application Support/` 或某个 worktree 里的
临时目录。这条规矩是踩出来的——第一版样板间只提交了 DB,DB 里的 `workspace_path`
指向一个被 `.gitignore` 挡在仓外的目录,技能库也依赖本机插件缓存;结果多人协作时
对方 clone 完什么都打不开。

```
examples/
├── aihot/                  ← 样板间:一个走完真实五阶段的项目
│   ├── bw-aihot.db         真实 SQLite(裁剪自真实日常库)
│   └── workspace/          该项目的真实工作区文件(44 个)
└── skill-libraries/        vendor 进仓的官方技能库(MIT,见其 README)
    ├── mattpocock-skills/  41 条
    └── superpowers/        14 条
```

## 加载(真的能打开,不是截图)

```bash
BW_DB=examples/aihot/bw-aihot.db cargo run -p app-desktop
```

深链直达某个面板(stderr 打 `[BW_OPEN]` 即渲染成功):

```bash
BW_DB=examples/aihot/bw-aihot.db BW_OPEN="aihot 日报" BW_PANEL=issues cargo run -p app-desktop
```

**请从仓根启动**:DB 里存的 `workspace_path` 是仓相对路径
(`examples/aihot/workspace`),换个工作目录启动就找不到工作区了。

打开会看到:31 个 Issue(29 Done/1 InProgress/1 Todo)、65 条技能(mattpocock 41 +
superpowers 14 + bw-standard 3 + 自建 7,**全部带真正文**)、73 个智能体、
真实健康信号——零 mock,是真跑出来的,不是摆拍。

## 它是怎么来的(可复现,不是手工捏的)

`bw-aihot.db` 是**真实日常库的一份裁剪副本**,由
[`crates/bw-app/examples/build_aihot_fixture.rs`](../crates/bw-app/examples/build_aihot_fixture.rs)
生成——那把裁剪刀留在仓里,就是为了让"样板间怎么来的"可复核、可重跑:

```bash
cargo run -p bw-app --example build_aihot_fixture
```

它走真实 `Command` 层做四件事:源库只读复制 → 除 aihot 外的项目全部
`DeleteProject`(隐私边界,日常库里的其它项目不进公开仓)→ `SetWorkspace` 把
工作区重接到仓内副本 → 用仓内 vendor 的技能库各导一次(正常结果 `imported=0 /
skipped=全部`,这恰好证明仓内副本与日常库同源)。最后 `VACUUM`——SQLite 删行只是
标记空闲页,公开发布的文件里被删项目的原始字节必须真的消失。

## 诚实的局限

- **工作区没有 git 历史**。`workspace/` 只有 44 个真实文件,不含那个工作区自己的
  `.git`(嵌套 `.git` 会变成 gitlink,把仓搞乱)。所以版本面板会如实显示"无历史",
  而不是伪造 32 次提交。真实提交历史的读回记录在
  [`iterations/PRACTICE-AIHOT.md`](../iterations/PRACTICE-AIHOT.md)。
- **打开一次就会写回一次**。`Boot`/`OpenProject` 会触发 `recompute_signals` 重算
  并落库,所以本地跑过样板间之后 `git status` 会显示 `bw-aihot.db` 有改动——
  内容没坏,是刷新了派生缓存与时间戳。不想提交这个改动就
  `git checkout -- examples/aihot/bw-aihot.db`。
- **技能库是快照,不跟上游同步**。见
  [`skill-libraries/README.md`](skill-libraries/README.md)。

## 相关材料(互补,不重复)

- [`iterations/PRACTICE-AIHOT.md`](../iterations/PRACTICE-AIHOT.md)——完整叙事:
  假设→动作→真实输出→结论,逐轮记录。
- [`iterations/AIHOT-EVIDENCE.json`](../iterations/AIHOT-EVIDENCE.json)——数字侧
  证据快照(不打开 DB 也能读的纯文本版本)。
