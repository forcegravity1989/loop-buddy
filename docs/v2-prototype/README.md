# docs/v2-prototype/ · V2 设计文档导读

> **30 秒导读**:本目录是 **V2 史实**(调度统一、同一仓多台 buddy 纳管)。对照当时怎么定的,可以读;**新的未决和 V4 特性不要往这里写**。2026-08-20 做过一次归档整理:初始意向录入篇 `roadmap.md` 已落地成两篇详细设计,搬去了归档;两篇详细设计仍在本目录,因为它们仍被现役文档引用为当前权威。遗留只认 [`../LEFTOVERS.md`](../LEFTOVERS.md);当前是 V3 修 bug、V4 规划,见 [`../releases.md`](../releases.md)。
>
> 看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md);代号查 [`../code-schemes.md`](../code-schemes.md);铁律与门禁见 [`../../CLAUDE.md`](../../CLAUDE.md)。工作流 skill:功能用 `buddy-feature-dev`,缺陷用 `buddy-bugfix`。

## 现在作数(留在本目录)

| 文件 | 是什么 | 状态 |
|---|---|---|
| [issue-dispatch-prompt-skill.md](issue-dispatch-prompt-skill.md) | **V2-①** 调度统一设计:所有 Issue 必带的 buddy 系统提示词与规范 + 按活选择的 Skill,两条独立资产线 | **已实现**;仍被 `docs/v3-prototype/cursor-agent-executor.md` 与 `docs/buddy/README.md` 引用为"Issue 开工时提示词怎么注入"的当前权威,不搬 |
| [same-project-multiple-workbenches.md](same-project-multiple-workbenches.md) | **V2-②** 最简多人设计:同一项目可被多台 Buddy 分别纳管(`.bw/project.toml` 正本 + 首到/后来者流程 + 回填 + 总览折线 + open Issue 读回) | **Phase A/B/V2-②-I 已落地**;§9 尾部仍有几条未关闭的 follow-up(见下「未关闭的欠账线索」),不搬 |

## 已归档(2026-08-20,纯历史 · 已被更详细设计取代)

- `roadmap.md` —— V1 维护运营周节奏 + V2 迭代功能初始意向(2026-08-10 用户口述录入)。内容已被上面两篇详细设计完全取代,搬去了 [`../archive/v2-prototype/roadmap.md`](../archive/v2-prototype/roadmap.md)。
- `open-design-embed-spike.md` —— Open Design 内嵌预研记录(2026-08-13 穿刺),本身自称"预研记录不是功能规格";对应能力(V3-OD-embed)已落地,搬去了 [`../archive/v2-prototype/open-design-embed-spike.md`](../archive/v2-prototype/open-design-embed-spike.md)。

## 未关闭的欠账线索(供排期参考,不是本文断言)

`same-project-multiple-workbenches.md` §9 checklist 里几条打了 `[ ]`(未完成)标记,本轮整理时未在 `docs/LEFTOVERS.md` 里找到对应条目:「后来者对齐到哪一步」的 UI 形态与留痕表、项目自有技能/队友/工作流的仓内正本、可重复执行的双 Buddy 验收剧本。是否需要补进 LEFTOVERS,留给用户判断。

## 与 V1 目录的关系

| 目录 | 管什么 |
|---|---|
| [`../v1-prototype/`](../v1-prototype/README.md) | V1 设计史实、终端重构、遗留清单 `LEFTOVERS.md`(含已拍板延期的「阶段默认 Skill / 系统提示词」条目) |
| **本目录** | V2 规划与后续 capability 设计归档;新功能设计往这里写,不要再堆进 V1 窗口号叙事 |

## 相关

- 产品命题:[`../../plan/07-product-proposition.md`](../../plan/07-product-proposition.md)
- 设计层事实源(仍有效的内核/铁律):[`../../plan/06-overall-alignment.md`](../../plan/06-overall-alignment.md)
- 遗留清单:[`../LEFTOVERS.md`](../LEFTOVERS.md)(含已落地的 V2-① 归正)
- V1 Issue 3(总览折线,被 V2-② Phase C 并入):[`../v1-prototype/issue3-overview-mockup.html`](../v1-prototype/issue3-overview-mockup.html)(视觉事实源;设计正文 `issue3-overview-refactor.md` 已归档,见 [`../archive/v1-prototype/issue3-overview-refactor.md`](../archive/v1-prototype/issue3-overview-refactor.md))
