# 司南 · Loop 工作台 — 设计包索引(前门)

> 多 Agent 对抗设计冲刺的产物。**先读这里导航,再读 [`DESIGN.md`](DESIGN.md)(唯一权威最终设计)。** 只设计、不实现。

## 一句话
做一个**多角色 TS Loop 工作台**:本质是「**注意力路由器(刃A)+ 判断力复利机(刃B)**」,不是 agent 监控大屏。对标 Mavis(虚拟办公室)/ Symphony(编排总控),但抄 job 不抄 UI。

## 怎么读(按角色)
- **想要结论** → [`DESIGN.md`](DESIGN.md) §0 定位 + §13 终审与未决风险。
- **想看怎么吵出来的** → [`blueteam-r1.md`](blueteam-r1.md)(R1 5×P0)→ [`blueteam-r2.md`](blueteam-r2.md)(终审「半活」)→ [`decision-log.md`](decision-log.md)(逐条裁决)。
- **想看界面** → 下面 4 块 mockup(在上级目录 `/Users/gravity/projects/`)。

## 设计稿 mockup(4 屏,司南设计系统,可直接浏览器打开)
| 屏 | 文件 | 角色 | 看点 |
|---|---|---|---|
| 决策队列首屏(刃A 锚屏) | `../sinan-loop-workbench-queue-v1.html` | 工程师(默认家) | VaR 排序决策队列 + 高危强制看diff/填理由 + 低危批量=产规则 + fleet rollup |
| 编排台 + 虚拟办公室 | `../sinan-loop-workbench-v1.html` | 工程师 | master-detail · 5 Agent 拟人办公室 · DAG 状态机 · Symphony 介入 |
| 度量看板 | `../sinan-loop-workbench-metrics-v1.html` | PM/QA | 吞吐↔护栏对照墙 · 复合健康灯 · false-autonomy · **刃B 复利曲线 + 规则账本** |
| 资产库 五元组 + Promotion 评审 | `../sinan-loop-workbench-assets-v1.html` | 资产 owner | MD+模型+Skills+权限+Gate 五元组 · base/override 版本治理 · **刃B 规则提升评审(四眼)** |
> 起点参考:`../sinan-control-plane-v1.html`(v1 作战室,只此一屏成稿)。

## 规格全文
- [`DESIGN.md`](DESIGN.md) — **权威**最终设计(双轮收敛 + 终审补丁 §4.6)
- [`pm-spec.md`](pm-spec.md) / [`ux-spec.md`](ux-spec.md) — R1 PM / 设计 独立全文
- [`rule-governance-spec.md`](rule-governance-spec.md) — 复利机规则治理专题(R3,⏳ 后台生成中)
- [`00-brief.md`](00-brief.md) — 三 agent 共享 ground truth

## 对抗轨迹
- **R1**:PM / 设计 / 蓝军 三个独立 sub-agent → 蓝军给 5×P0 + 1 方法论 P0。
- **R2**:编排者收敛(补刃B复利机)→ **全新独立蓝军终审:「半活」**——刃A 可进实现,刃B 复利机补齐 §4.6 三条硬前置前不得进实现。
- **R3(进行中)**:三条新攻击轴并行 ——
  - ⏳ `rule-governance-spec.md`:把复利机从"半活"做到"活"(定主体/定仲裁/定真值 + R-6/R-7 数学)。
  - ⏳ `security-redteam.md`:信任边界/注入/滥用威胁建模(用户上报 + 竞品雷达 = 不可信输入)。
  - ⏳ `cost-redteam.md`:单位经济学(10k agent + Opus + shadow 双跑的成本)。

## 当前裁定 & 未决风险
**「半活」** —— 刃A 路由侧已活;刃B 复利机待 R3 收口。未决风险见 [`decision-log.md`](decision-log.md) R-1~R-8(R-6「复利机=换名 O(n)?」是 P0,R3 正在解)。

## 下一步(R3 收口后)
1. 把 R3 三轴结论并入 `DESIGN.md` §13 + 起 BlueTeam3 终审复利机治理。
2. 视结论补第 5 屏:复利·规则账本全屏(刃B 治理面)。
3. 刃A 侧可先行进高保真 / TS 实现准备。

> 流程诚实:R2 两个 worker agent 曾连续 infra stall,R2 收敛由编排者代行,并用全新独立蓝军补回对抗独立性(见 `decision-log.md` 流程注记)。
