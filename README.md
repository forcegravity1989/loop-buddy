# 司南 · Loop 工作台

> 多 Agent 对抗设计冲刺的产物。**只设计,不实现。** 仓库由「HTML 设计稿 mockup + 设计文档」两部分组成。

## 这是什么
一个**多角色 TS Loop 工作台**的概念设计:本质是「**注意力路由器(刃A)+ 判断力复利机(刃B)**」,对标 Mavis(虚拟办公室)/ Symphony(编排总控),但抄 job 不抄 UI。

- **铁律一:只设计,不实现。** TS 只体现为「组件架构 + 类型/接口 + 状态归属」,不含实现代码;HTML 属于「设计稿」。
- **铁律二:工作台必须同时服务多角色**(PM/QA、工程师、资产 owner),不是单角色工具。

## 仓库结构
```
sinan-control-plane-v1.html          # v1 作战室(监督视图,工程负责人视角)——起点参考
sinan-loop-workbench-queue-v1.html   # 决策队列首屏(刃A 锚屏,工程师默认家)
sinan-loop-workbench-v1.html         # 编排台 + 虚拟办公室
sinan-loop-workbench-metrics-v1.html # 度量看板(PM/QA)
sinan-loop-workbench-assets-v1.html  # 资产库 五元组 + Promotion 评审
sinan-workbench-design/              # 设计文档(对抗设计全过程)
├── README.md        # 设计包前门索引 —— 先读这里
├── DESIGN.md        # 唯一权威最终设计
├── 00-brief.md      # 三 agent 共享 ground truth
├── pm-spec.md       # PM 全文(R1)
├── ux-spec.md       # 设计全文(R1)
├── blueteam-r1.md   # 蓝军 R1(5×P0)
├── blueteam-r2.md   # 全新独立蓝军终审:「半活」
├── decision-log.md  # 逐条裁决(R-1~R-8)
├── rule-governance-spec.md  # 复利机规则治理专题(R3)
├── security-redteam.md      # 信任边界/注入/滥用威胁建模(R3)
├── cost-redteam.md         # 单位经济学(R3)
└── premortem-human-factors.md
```

## 怎么读
- **想要结论** → [`sinan-workbench-design/README.md`](sinan-workbench-design/README.md) → [`DESIGN.md`](sinan-workbench-design/DESIGN.md) §0 定位 + §13 终审与未决风险。
- **想看界面** → 直接浏览器打开上面 4 个 `sinan-loop-workbench-*.html` mockup。
- **想看怎么吵出来的** → `blueteam-r1.md` → `blueteam-r2.md` → `decision-log.md`。

## 当前裁定
**「半活」** —— 刃A 路由侧已活;刃B 复利机待 R3 收口。详见 [`decision-log.md`](sinan-workbench-design/decision-log.md)。

## License
[Apache-2.0](LICENSE)
