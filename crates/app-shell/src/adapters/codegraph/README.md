# 适配模块 · codegraph

**借自哪个项目/文件**:`@colbymchenry/codegraph`(npm 全局安装的命令行工具),
版本以仓根 `scripts/codegraph-version` 为准(今天是 1.5.0,CI 钉的就是它)。

**借了什么**:

- `codegraph files -j` —— 每个文件的 `path` / `language` / `nodeCount` / `size`,
  本模块按 `size` 排序取前若干行当「大文件榜」。字段原样透出,不加工。
- 「装没装 / 建没建索引」这两个判据:命令在不在 PATH、仓里有没有 `.codegraph/`。

**没借什么**:

- **死代码判定**。`callers` / `impact` 这两条命令在本仓这种大量 `dyn Trait`
  动态派发的代码里会漏边(预研实测),「零调用者」不能当死代码结论。要用
  这类数字做微重构,人必须复核。
- `codegraph explore` 的模块依赖概览。它有没有稳定的结构化输出还没核实过,
  首版不接,不猜一个解析格式出来。
- 任何形式的缓存。每次打开页签就是一次全新的子进程调用 —— 和「对账是纯读
  操作不需要缓存」同一个取舍。
