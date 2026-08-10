# V2 路线与初始信息录入

> **30 秒导读**:这是 V2 的**初始节奏与功能意向**录入(2026-08-10 用户口述落盘),给后续 `buddy-feature-dev` 会话当起点。**现在作数**;细节设计未齐前不假装已定 API/UI。V1 已合入 main——日常先稳住托管与问题收集,周四出 V1.x 修 bug/易用性并推广;V2 两条能力线另开设计再动手。

---

## 1. V1 的维护运营(周节奏)

目标:把 buddy 真正用在 **benchmark、omhwcc、运营** 三类 workbench 托管上,边用边收问题,再收口成可推广版本。

| 时段 | 做什么 |
|---|---|
| **周一到周三** | 完成 benchmark、omhwcc、运营 的 workbench **托管使用支撑**与**问题收集**(实跑 > 猜改) |
| **周四下午** | 出 **V1.x** 版本:解决 V1 BUG 与易用性;组会推广,**全面托管** |

约束(沿用仓纪律):

- Done 永不自动 / 信号只能从数据推导 / 同一件活不重复记账 / UI 无关内核——见 `CLAUDE.md`。
- 修缺陷走 `buddy-bugfix`;V1.x 易用性若变成新形态再走 `buddy-feature-dev`。
- 问题与延期项继续可记进 `docs/v1-prototype/LEFTOVERS.md`,或本目录后续「运营台账」文(未建则先写 LEFTOVERS)。

---

## 2. V2 的迭代功能(意向)

### 2.1 调度逻辑简化(催熟 buddy 调度 issue)

把 issue 交付调度收成两块可维护的东西(与用户 2026-08-10 拍板、已记入 V1 `LEFTOVERS` 的 V2 条一致):

1. **系统提示词**  
   - 一份 buddy 大提示词(契约 / 铁律 / 项目上下文)。  
   - **渐进性加载** buddy 相关约束文档(例如处理指标时再加载 metrics / connectors 规范),避免一股脑塞进、也避免无规范导致托管对不齐。

2. **默认装载的技能(至少三处)**  
   - **找指标**  
   - **绑数据**  
   - **构建板块**  
   - 选了对应板块 = 装载该板块默认 skill;agent 小队调度本身也视为 skill(不再指望 buddy 用旧阶段循环脚本去调度 issue 侧多 agent)。

设计时必读史实(勿直接当实现规格):

- `docs/v1-prototype/issue2-all-issues-terminal-runs.md` — issue 全走嵌入终端、多 agent 转 prompt 的收口决定。  
- `docs/guide/buddy-guide.html` m4 — 「默认系统提示词 / 默认 skill」留口。  
- `docs/v1-prototype/LEFTOVERS.md` — 「V2 · 阶段默认 Skill / 系统提示词与规范手册」。

**未决(录入时不擅定)**:默认 skill 正本放 Hub 还是 `docs/skills/`;渐进加载的文档清单与触发条件;构建默认 skill 是否覆盖五阶段其余三角色——开 `buddy-feature-dev` 时再 grill 对齐。

### 2.2 调研 workbuddy,加持最简的多人机制

意向:**项目被多人 workbench 纳管 + 查看**。

- 先调研 workbuddy(对照可借鉴点与 buddy 反命题边界)。  
- 「最简」= 纳管与查看优先,不默认做成团队协作平台(群聊 / 收件箱 / 审批流等仍属 `plan/07` 反命题,除非明确改命题)。

**未决**:多人身份与权限模型、数据是本地库同步还是只读远端视图、与单人 Builder 命题如何共存——调研笔记落本目录后再定 delta。

---

## 3. 建议的后续落盘顺序

1. 运营周:问题进 LEFTOVERS 或本目录运营台账 → 周四 V1.x 收一组可验证修复。  
2. V2-2.1:单独开设计篇(如 `issue-dispatch-skill-prompt.md`)→ scope delta → 再开发。  
3. V2-2.2:workbuddy 调研笔记 → 最简多人 delta → 再开发。

---

## 4. 变更记录

| 日期 | 什么 |
|---|---|
| 2026-08-10 | 用户口述初始录入:V1 周节奏 + V2 调度简化 + 最简多人 |
