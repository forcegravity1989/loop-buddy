# docs/v2-prototype/ · V2 设计文档导读

> **30 秒导读**:本目录是 **V2 史实**（调度统一、同一仓多台 buddy 纳管）。对照当时怎么定的，可以读；**新的未决和 V4 特性不要往这里写**。遗留只认 [`../LEFTOVERS.md`](../LEFTOVERS.md)；当前是 V3 修 bug、V4 规划，见 [`../releases.md`](../releases.md)。
>
> 看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md);代号查 [`../code-schemes.md`](../code-schemes.md);铁律与门禁见 [`../../CLAUDE.md`](../../CLAUDE.md)。工作流 skill:功能用 `buddy-feature-dev`,缺陷用 `buddy-bugfix`。

## 现在作数

| 文件 | 是什么 | 状态 |
|---|---|---|
| [roadmap.md](roadmap.md) | V1 维护运营周节奏 + V2 迭代功能初始意向(调度简化 / 多人最简) | 初始意向源;V2-①/② 已各自落设计篇 |
| [issue-dispatch-prompt-skill.md](issue-dispatch-prompt-skill.md) | **V2-①** 调度统一设计:所有 Issue 必带的 buddy 系统提示词与规范 + 按活选择的 Skill,两条独立资产线 | **已实现**(commit `4073ae2`..`8f35b6b`,未 push;行为层 E2E defer 用户) |
| [same-project-multiple-workbenches.md](same-project-multiple-workbenches.md) | **V2-②** 最简多人设计:同一项目可被多台 Buddy 分别纳管(`.bw/project.toml` 正本 + 首到/后来者流程 + 回填 + 总览折线 + open Issue 读回) | **Phase A/B 已实现**;**V2-②-I** 已落地(含同步收起:远端已关且本机未完成→Cancelled;本机 Done 保留) |

> **V2 实施进度**:V2-①(调度简化)已实现;V2-② Phase A(多人闭环)/B(回填)/C(总览折线)与 **V2-②-I**(仓平台 open Issue 读回本地)已落地。所有 V2 commit 均在 `v1` 分支,**未 push**。

## 与 V1 目录的关系

| 目录 | 管什么 |
|---|---|
| [`../v1-prototype/`](../v1-prototype/README.md) | V1 设计史实、终端重构、遗留清单 `LEFTOVERS.md`(含已拍板延期的「阶段默认 Skill / 系统提示词」条目) |
| **本目录** | V2 规划与后续 capability 设计归档;新功能设计往这里写,不要再堆进 V1 窗口号叙事 |

## 相关

- 产品命题:[`../../plan/07-product-proposition.md`](../../plan/07-product-proposition.md)
- 设计层事实源(仍有效的内核/铁律):[`../../plan/06-overall-alignment.md`](../../plan/06-overall-alignment.md)
- 遗留清单:[`../LEFTOVERS.md`](../LEFTOVERS.md)（含已落地的 V2-① 归正）
- V1 Issue 3(总览折线,被 V2-② Phase C 并入):[`../v1-prototype/issue3-overview-refactor.md`](../v1-prototype/issue3-overview-refactor.md)
