# 司南 · Loop 工作台

> 多 Agent 对抗设计冲刺的产物。**只设计,不实现。** 仓库由「HTML 设计稿 mockup + 设计文档」两部分组成。

## 这是什么
一个**多角色 TS Loop 工作台**的概念设计:本质是「**注意力路由器(刃A)+ 判断力复利机(刃B)**」,对标 Mavis(虚拟办公室)/ Symphony(编排总控),但抄 job 不抄 UI。

- **铁律一:只设计,不实现。** TS 只体现为「组件架构 + 类型/接口 + 状态归属」,不含实现代码;HTML 属于「设计稿」。
- **铁律二:工作台必须同时服务多角色**(PM/QA、工程师、资产 owner),不是单角色工具。

## 仓库结构

### 当前主版本(中文极简 · v2)
```
📌 sinan-workbench-cn-v2.html        # 新·全 5 屏工作台(中文·大白话·零黑话·minimalist-ui)
   ├── 概览 · 谁需要你 → 什么在自主跑(汇总) → 最近动态
   ├── Loop 详情 · 它做什么 / 走到哪 / 参与的智能体 / 需要你的地方
   ├── 智能体 · 1,284 个按职能+模型汇总(不平铺)
   ├── 概况 · 本周平静视图:自主闭环/你介入/问题趋势/返工率
   └── 资产库 · Loop 蓝图 / 智能体配置 / 技能库

sinan-workbench-design/redesign-contract.md  # 重设计验收尺 + 黑话→白话词表
```

### 历史参考(v1 + 对抗过程)
```
sinan-control-plane-v1.html          # v1 作战室(监督视图)——历史参考
sinan-loop-workbench-queue-v1.html   # 决策队列首屏(v1)——历史参考
sinan-loop-workbench-v1.html         # 编排台 + 虚拟办公室(v1)——历史参考
sinan-loop-workbench-metrics-v1.html # 度量看板(v1)——历史参考
sinan-loop-workbench-assets-v1.html  # 资产库(v1)——历史参考
sinan-workbench-minimal-v1.html      # 英文极简样板(v1)——历史参考

sinan-workbench-design/              # 设计文档 · 对抗设计全过程 · 读这些理解决策
├── README.md        # 设计包前门索引
├── DESIGN.md        # 唯一权威设计(v2,四轮收敛 + §4.9 BT3 补丁)
├── implementation-gate.md  # go/no-go 闸门(M0a/M0b + 硬前置)
├── R3-VERDICT.md    # R3 关键产出 · 安全/人因/治理 P0 增补
├── 00-brief.md      # 三 agent 共享 ground truth
├── pm-spec.md       # PM 全文(R1)
├── ux-spec.md       # 设计全文(R1)
├── blueteam-r1.md   # 蓝军 R1(5×P0)
├── blueteam-r2.md   # 独立蓝军终审 R2
├── blueteam-r4.md   # 独立蓝军终审 R4
├── decision-log.md  # 逐条裁决(R1~R4)
├── rule-governance-spec.md
├── security-redteam.md
├── cost-redteam.md
└── premortem-human-factors.md
```

## 怎么读

**🟢 快速上手(2min)**
1. 在浏览器打开 `sinan-workbench-cn-v2.html`——中文、大白话、五屏完整工作台
2. 点顶部导航:概览 → Loop详情 → 智能体 → 概况 → 资产库

**🟡 理解设计决策(30min)**
- [`sinan-workbench-design/README.md`](sinan-workbench-design/README.md) → [`DESIGN.md`](sinan-workbench-design/DESIGN.md) §0 定位
- [`redesign-contract.md`](sinan-workbench-design/redesign-contract.md)——重设计验收标准 + 黑话清单

**🔴 深入对抗过程(2h)**
- 对抗史:R1/R2/R4 蓝军终审 → `decision-log.md`
- 风险单:R-9(安全 P0 注入链)详见 [`R3-VERDICT.md`](sinan-workbench-design/R3-VERDICT.md)

## 当前裁定

**v2 UI 重设计已完成** ✅
- 新:5 屏工作台,中文极简,零术语黑话
- 验收:通过设计闸 Gatekeeper 两轮复检(PROCEED → SHIP)
- 风格:对标 Claude Code / Cloud Cowork / Workbody,克制单色 + 大汇总

**工程侧风险** 🔴
- 见 [`R3-VERDICT.md`](sinan-workbench-design/R3-VERDICT.md):R-9 安全 P0(注入链)、R-10 人因 P0 等待实现前硬闸

## 进度
- **R1→R4 四轮对抗设计已完成** + R3 关键产出(trustTier / 人因 / 治理)
- **UI 重设计 v2 已完成** · 中文极简 · 零黑话 · Gatekeeper 二度通过
- **下一步** → 工程侧补 R-9/R-10 的硬前置方案再进实现

## License
[Apache-2.0](LICENSE)
