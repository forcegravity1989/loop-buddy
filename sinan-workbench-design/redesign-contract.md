# Loop 工作台 · 重设计契约(redesign-contract.md)

> 给「设计评审 teammate」的验收尺 + 跨模块一致性基线。依据两个 skill:**minimalist-ui**(高端极简)+ **high-end-visual-design**。本契约是 go/no-go 的客观标准。

## 1. 语言(中文平台)
- **全中文界面**;术语一律大白话,普通人秒懂。
- **禁止上界面的黑话**(只活在工程文档,不进 UI):复利机 / 刃A / 刃B / trustTier / 毕业三闸 / value-at-risk / decision leverage / 护栏一票否决 / Symphony / 编排台 / 虚拟办公室 / Mavis / 状态机 / DAG。
- **黑话 → 白话 词表(强制)**:
  | 内部概念 | 界面用词 |
  |---|---|
  | Loop | Loop(产品词,保留) |
  | Agent | 智能体 |
  | needs human / escalation | 待你处理 / 需要你定夺 |
  | running autonomously | 自主运行中 |
  | step in / take over | 介入 / 我来接手 |
  | approve / reject | 批准 / 退回 |
  | gate(受保护边界) | 需要人点头(给出大白话理由) |
  | metrics / 度量看板 | 概况 / 本周 |
  | blueprint | 蓝图(模板) |
  | model tier | 用的模型 |
  | 工序时间线 / state machine | 它走到哪一步 |

## 2. 视觉(严守 minimalist-ui)
- 暖白底 `#F7F6F3` / 白卡 `#FFF`;炭灰字 `#2F3437`,次要 `#787774`,极淡描边 `1px #EAEAEA`;圆角 8–12px。
- 字体:正文/UI = `PingFang SC`(系统、干净、高端);标题 = `Newsreader + Songti SC` 衬线;数字/ID/时间 = `JetBrains Mono`。
- **颜色是稀缺资源**:仅「待你处理 / 异常」用暖红 `#FDEBEC`/`#9F2F2D`;「运行 / 正常」用淡绿 `#EDF3EC`/`#346538`;「等待」用淡黄 `#FBF3DB`/`#956400`;「中性信息」用淡蓝。
- **禁**:渐变、重阴影(阴影 opacity < .05)、emoji、霓虹、大色块 hero、pill 形大按钮。大留白、安静、document 式(对标 Claude Code / Cowork)。

## 3. 模块清单(全部重设计)
1. **概览(首页)**:待你处理 → 自主运行(**汇总**,绝不平铺上千)→ 最近动态。
2. **Loop 详情**(已认可方向 A):它在做什么 / 它走到哪一步(横向工序条)/ 参与的智能体(简列,**无办公室戏剧化**)/ 需要你的地方。
3. **智能体**:在岗花名册——各干什么、用什么模型、在多少 Loop 里(汇总视角,几千个被聚合)。
4. **概况(度量)**:一屏平静的「本周」——自主闭环数 / 你介入次数 / 开放问题趋势 / 合并后返工率。**不堆仪表盘、不放复利曲线。**
5. **资产库**:Loop 蓝图 / 智能体配置 / 技能库,干净列表。

## 4. 验收(评审 teammate 据此判 go/no-go)
- [ ] 零黑话——逐标签检查,普通人能懂。
- [ ] 严守 minimalist-ui——配色 / 字体 / 1px 描边 / 留白 / 无渐变无重阴影无 emoji。
- [ ] 上千智能体/Loop 被「汇总」而非平铺。
- [ ] 一致性——5 模块同一套 token、同一套语言、同一个导航。
- [ ] 高端 & 安静——像在「看」而非「操控仪表盘」。
- 任一项不达标 → 判 revise 并给出具体修改项;全过 → 判 proceed。
