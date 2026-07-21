# design/ — Builders 工作台设计稿件归档

本目录收录 Builders 工作台项目管理体系的全部交互原型稿（`.dc.html`）、配套运行时与设计评审材料，按**文档族**分子目录存放——每个子目录内是同一设计主题的多个版本迭代，而不是互不相关的文件堆放。

这里是**待评审的设计探索**。如果要找"真实跑出来的验证报告/演示"，见 [`../verification/`](../verification)。

## 怎么打开

`.dc.html` 是自包含的单文件交互原型，靠同级的 [`support.js`](support.js)（dc-runtime）渲染，用浏览器直接打开子目录下的 `.dc.html` 即可——相对路径已配置为 `../support.js`，不需要额外起服务。

## 目录结构

| 目录 | 主题 | 版本脉络 |
|---|---|---|
| [`project-management-wizard/`](project-management-wizard) | **核心向导**——新建项目分步引导（起名→洞察→竞品→北极星→原型→先行指标→上线）。仓库 `plan/` 下 Rust 重写方案唯一的原型依据（见 `plan/00-PLAN.md` 等引用） | 基础版 → v2 → v3 → v4。v4 是五阶段五方法论落地，设计决策记录见同目录 [`v4-upgrade-plan.md`](project-management-wizard/v4-upgrade-plan.md) |
| [`system-refactor-plan/`](system-refactor-plan) | 体系重构方案——角色分工与项目周期的方法论设计，是向导 v4 的设计依据 | 角色与周期（初版） → v2 五阶段五方法论 |
| [`creation-flow/`](creation-flow) | 创建流程——「新建项目」交互探索，两版内都有「进入工作台 →」按钮跳转到 `project-management-wizard/` | v3 Claude 式向导（对话式） → v4 卡片流（改版探索） |
| [`ops-page-reduction/`](ops-page-reduction) | 运营页减法方案——运营看板信息精简探索 | 基础版 → v2 四阶段轴 |
| [`explorations/`](explorations) | 单版本探索稿，暂无后续迭代：洞察链路重构方案 / 看板与创建-主轴方案探索 | 各自独立一版 |
| [`role-maps/`](role-maps) | 角色分工能力地图：`everything-claude-code-roles.html`（ECC 全量版）/ `omc-roles.html`（OMC 版） | 各自独立文档，非同一稿的演进 |

`screenshots/` 是设计评审截图存档（提案/改动/编辑/定位等视图），未被任何 `.dc.html` 引用，纯人工参考材料，不随原型改动。

## 为什么每个子目录里留着多个版本，而不是只留最新版

这些 `.dc.html` 是**交互原型**，不是源码——判断一版设计好不好，要靠在浏览器里实际点开、走一遍交互，而不是靠读 diff。旧版本留在工作区，是为了能随时并排打开新旧两版对比手感，这件事 `git log`/`git show` 做不到（它只给你文本差异，给不了可交互的页面）。

所以这里的“版本”不是代码意义上的“历史记录，只有最新的活着”，而是“设计探索的不同快照，都可能被重新翻出来参考或对比”。每次新增版本时，正常追加文件即可，不需要因为“已经有更新的版本了”而删旧版本——除非确认某一版彻底作废、不会再被拿来对比。

（`creation-flow/` 是个更极端的例子：v3、v4 根本不是同一套结构的迭代，而是两种不同交互范式的并行探索，见下方版本脉络说明——这类"分叉"更应该常驻，而不只是暂存。）

## 依赖关系提示（改动前必读）

- 所有 `.dc.html` 都通过 `<script src="../support.js">` 加载同一份运行时。**不要**把某个 `.dc.html` 单独移出其子目录，否则该引用会失效；新增文档族子目录时也要照此改相对路径。
- `creation-flow/` 下两个文件的「进入工作台 →」按钮硬编码链接到 `project-management-wizard/Builders工作台-项目管理向导 v2.dc.html`（注意：链接目标固定在 v2，并未随向导升级到 v3/v4 同步更新——这是原稿本身的状态，不是本次搬迁引入的问题）。若该文件改名或移动，需要同步更新这两处 `href`。
