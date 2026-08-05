# V1 产品化 · 遗留问题汇总

> V1 三个窗口（纳入项目 / 找指标·绑数据 / 总览重构）实践过程中冒出、但**不在当前窗口解**的问题。
> 每条标产生窗口（W1/W2/W3）+ 现象 + 未决点 + 处置。三窗口一把合入后，把这里的条目转成 issue 挂到库上。

---

## W1-1 · 创建时 buddy 自动写并 push 用户项目仓提交

**产生窗口**：W1 纳入项目（`docs/v1-prototype/issue1-onboard-simplify.md`）

**现象**：`CreateProject` / `CompleteCreation` 在用户项目的 owned workspace 自动 `commit_file` 写两类文件，并在 `CompleteCreation` 末 `push_head` 推远端：
- `PROJECT.md` 章程（`docs(bw): 项目章程 · 开篇` + `… · 完成创建`，两次提交）——`crates/bw-app/src/lib.rs` `write_charter`
- `.claude/standards/{agent,skill,workflow,cron}-standards.md` 四份组件标准（`docs(bw): 模板能力 · 组件标准文件`）——`write_component_standards`

buddy 在自己 workspace_path（`BW_WORKSPACES` 下的 clone）里提交，再 push 到用户项目远端 main。

**未决点**：
1. **必要性 / 健壮性**——buddy 动用户项目 git 历史是否必须？章程 + 组件标准该自动写进仓，还是该由用户主动生成 / opt-in？`docs(bw):` 提交约定是否撞用户项目自有的 commit 规范（用户项目可能有自己的 conventional-commits / 签名要求）？
2. **worktree 感知**——buddy 在自己的 workspace clone 提交并 push；用户若在项目的独立 worktree 工作，需 `git pull` 才能感知这些自动提交，存在同步感知缺口（用户不知道 buddy 往 main 推过东西）。
3. **PR 独立性**——三件套（竞品分析 / 找指标 / 绑数据）各产 PR；charter + standards 已在 base（main）上，PR 从该 base 分支基本不碍独立性。但 buddy `push_head` 与用户 worktree 并行 push 同一 main 可能产生分叉 / 冲突 / 强推风险。

**处置**：W1 不解，暂留。待三窗口合入后与各窗口遗留汇总转 issue。

**事实源**：`crates/bw-app/src/lib.rs`（`write_charter` L7829 / `write_component_standards` L7855 / `push_head` 调用 L5522）。
