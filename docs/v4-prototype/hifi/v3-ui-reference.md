# V3 界面 HTML/CSS 复用参考

> **30 秒导读**:子代理 2026-08-18 从 `crates/app-desktop/src`(theme.rs + screens/*.rs)抄出来的 V3 界面结构与 inline 样式,给 V4 高保真原型和后面的新壳「不推倒 V3」用。它是抄录,不是设计决定;V3 代码改了以它为准重抄。

来源:`crates/app-desktop/src/theme.rs` + `screens/{chrome,project_rail,wall,op,skill_hub,agent_hub,cron_hub,connector_hub,settings_hub,create,component_detail}.rs`(Dioxus 0.7 + inline style,渲染成 wry WebView 里的真实 HTML/CSS,所以下面的 inline style 字符串就是可以直接照抄的 CSS)。

---

## 1. Token 表(`theme.rs`,全部原值)

**颜色**
```
PAPER       #EFEBE2   底色(暖纸)
RAIL_BG     #E9E3D7   图标栏底色
CLAY        #C5654A   品牌色 / 主按钮 / 强调
CARD        #FBFAF6   卡片底色
CARD_ALT    #F4F0E7   卡片备用底色(强调条/深色卡)
BORDER      #E2DCCF   边框
BORDER_DEEP #DBD4C5   深一档边框(input)
INK         #23211C   正文
INK_2       #57534A   次要文字
INK_3       #8C867A   三级/说明文字
INK_4       #A19B8D   最弱/占位文字
AGENT       #5A4E7A   Agent 紫(agent 消息/chip)
ALERT_DEEP  #A33D29   警示深红(阻塞/失败/删除确认)
```
signal 三态色(不在 theme.rs,来自 `ui::signal_color`,按 `bw_core::model::Signal`)代码中常量用法:Green 隐身(卡片默认不强调)、Amber、Red、Unknown=灰。实际取值需查 `ui` crate,但 UI 侧一律用 `ui::signal_color(signal)` 返回一个十六进制字符串,不硬编。

**阴影**
```
SHADOW  0 8px 26px rgba(35,33,28,.08)
```

**字体**
```
SERIF  'Noto Serif SC','Songti SC','STSong','SimSun',serif      -- 标题/项目名/大数字
SANS   'Noto Sans SC','PingFang SC','Hiragino Sans GB','Microsoft YaHei',sans-serif  -- 正文默认
MONO   'JetBrains Mono','SF Mono',Menlo,Consolas,monospace       -- 数字/代号/路径/时间戳
```

**全局 CSS(`GLOBAL_CSS`,原样抄)**
```css
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; height: 100%; }
::selection { background: #E7CFC4; }
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { background: #D8D1C2; border-radius: 6px; border: 3px solid #EFEBE2; }
::-webkit-scrollbar-track { background: transparent; }
button { font-family: inherit; }
input, textarea, select { font-family: inherit; color: #23211C; }
textarea { resize: vertical; }
input:focus, textarea:focus { outline: 1.5px solid #C5654A; outline-offset: 0; }
```

**原子样式函数(直接对应 CSS class)**
```
dot(color, size)  → width/height:{size}px;border-radius:50%;background:{color};display:inline-block;flex:none;
card()            → background:#FBFAF6;border:1px solid #E2DCCF;border-radius:10px;box-shadow:0 8px 26px rgba(35,33,28,.08);
chip(bg, fg)      → display:inline-block;padding:2px 8px;border-radius:6px;background:{bg};color:{fg};font-size:11px;line-height:16px;white-space:nowrap;
btn_primary()     → cursor:pointer;background:#C5654A;color:#FFF;border:none;border-radius:8px;padding:10px 22px;font-size:14px;font-weight:500;
input()           → width:100%;background:#FFFDF8;border:1px solid #DBD4C5;border-radius:8px;padding:9px 11px;font-size:13px;line-height:1.55;
label()           → font-size:12px;color:#8C867A;margin:0 0 6px;display:block;
```
圆角总体节奏:卡片 10px、按钮/输入框 7-8px、chip 6px(圆形 15px 用在 create.rs 的问答式 chip)、小方框(DoD checkbox)4px。

---

## 2. 外壳(chrome.rs + project_rail.rs)

### 2.1 全局图标栏 `IconRail`(chrome.rs:70-118)
- 宽 **64px**,`flex:none`,底色 `RAIL_BG`,右边框 1px `BORDER`,纵向 flex,`padding:14px 0;gap:4px`,居中对齐。
- 顶部 Logo 方块:34×34px,圆角 9px,底色 CLAY,白字「B」,serif 700 17px,margin-bottom 10px。
- 下方 10 个 `RailIcon` 按钮(工作台/SkillHub/AgentHub/Routines/CronHub/Connectors/Knowledge/Activity/通知/设置):每个 40×40px 圆角 9px 按钮,内嵌 24×24 viewBox 的 stroke SVG(19×19 显示,stroke-width 1.7,圆头圆角连接)。选中态:背景 `#DED5C2`、描边色 `INK`;未选中:背景透明、描边色 `INK_3`。

骨架:
```html
<div style="width:64px;background:#E9E3D7;border-right:1px solid #E2DCCF;display:flex;flex-direction:column;align-items:center;padding:14px 0;gap:4px;">
  <div style="width:34px;height:34px;border-radius:9px;background:#C5654A;color:#fff;display:flex;align-items:center;justify-content:center;font-family:serif;font-weight:700;font-size:17px;margin-bottom:10px;">B</div>
  <button style="width:40px;height:40px;border:none;border-radius:9px;background:#DED5C2;/*或 transparent*/;cursor:pointer;display:flex;align-items:center;justify-content:center;"><svg>...</svg></button>
  <!-- ×10 -->
</div>
```

### 2.2 项目内左栏 `ProjectRail`(project_rail.rs:124-225)
- 打开某个项目后,IconRail 右侧再插一条 **198px** 宽的栏,底色同 `RAIL_BG`,右边框,`padding:16px 12px`,可纵向滚动。
- 顶部小标题「本项目组件」:mono 10.5px、字距 0.06em、`INK_3`。
- 下面 5 个 `RailGroup`(技能/智能体/工作流/定时/连接器),每组:
  - 组头一行 `justify-content:space-between`:label(12.5px 500 INK_2) + 数量(11px INK_3)。
  - 空态:11px INK_3 提示语「本项目还没有自建的…」。
  - 非空:纵向 `gap:3px` 的纯文字行(11.5px,INK_2,单行省略号,点击触发 `on_pick` 打开详情,不是按钮元素,是可点 div)。
  - 组底:若有「共享/全局/全部项目」计数 >0,加一行虚线上边框(`border-top:1px dashed BORDER`)的小字「+ N 共享」。

这一栏是纯文字列表 + 分组标题,没有卡片、没有图标,是全屏最朴素的一块。

### 2.3 无独立顶栏组件
chrome.rs 没有单独的「顶栏」组件——项目内的顶栏是 op.rs 的 `TopBar`(见下)。全局 Hub 页面(SkillHub 等)自己在内容区顶部画 mono 小标题,没有横跨全宽的应用顶栏。

### 2.4 启动/错误态
`BootFrame`:全屏居中「正在打开本地工作台…」(INK_3 13px)。`FatalFrame`:`card()` 居中卡片,标题「无法启动」红色加粗+正文。`Toast`(底部悬浮错误条):`position:fixed;left:50%;transform:translateX(-50%);bottom:22px;background:#A33D29;color:#FFF;border-radius:9px;padding:10px 14px;font-size:13px;` + 「关闭」(浅粉字 #F3D9CF)。

---

## 3. 项目墙(`wall.rs`)

- 容器:`max-width:1060px;margin:0 auto;padding:44px 40px 60px;`。
- 顶部小 Logo 行:26×26 clay 方块「B」+ 「Builders' 工作台」(13px INK_2)。
- `<h1>` 标题「我的项目」:serif 600 30px,margin-bottom 6px。
- **本机环境条**(`LocalEnvBar`,wall.rs:66-96):`card()` + `padding:14px 18px;margin-bottom:18px;`。一行 flex-wrap:
  - mono 10px 大写字距标签「本机环境」(INK_3)
  - 「claude · {状态文字}」「codehub-cli · {状态文字}」——颜色随 EnvCheck 枚举变(Unknown=INK_3灰 / Probing=INK_2 / Ok=INK黑 / Fail=ALERT_DEEP红)
  - 右侧(`margin-left:auto`)「测一下」按钮:透明底、CLAY 描边 1px、CLAY 字、圆角 6px、padding 5px 11px、11.5px;探测中禁用并显示「探测中…」。
  - 下方一段 12px INK_3 提示小字(换行说明装环境的步骤)。
- **健康概览条**(`HealthOverviewBar`,wall.rs:104-164):同样 `card()` 一行,mono 标签「健康概览」+ 按 Signal 分组的计数(绿=「N 平稳」灰字仅计数、黄=「需要关注 N」、红=「阻塞 N」、Unknown=「无数据 N」灰字),每组前面一个 8px 圆点(`theme::dot`);无项目时显示「尚无项目」;右侧 `margin-left:auto` 显示「共 N 个项目」。
- 说明文字一行(13px INK_2)。
- **卡片网格**:`display:grid;grid-template-columns:repeat(2, minmax(0,1fr));gap:18px;`(固定两列)。

### 3.1 `ProjectCard`(wall.rs:166-260)——元素顺序 + inline style
```html
<div style="{card()} padding:18px 20px;cursor:pointer;" onclick="OpenProject">
  <!-- 顶行:阶段chip + 周期标签 + 开放issue徽 + 灯 + 删除× -->
  <div style="display:flex;align-items:center;gap:6px;margin-bottom:12px;">
    <span style="{chip}">{phase_label}</span>                 <!-- 运行中=绿系#E7EDE2/#4A5E42,否则=橙系#F2E4DD/#B0503A -->
    <span style="font-size:11px;color:INK_3;">{cycle_label}</span>
    <span style="font-size:11px;color:CLAY;border:1px solid CLAY;border-radius:10px;padding:1px 8px;" v-if="open_issues>0">⚑ {n} 开放</span>
    <span style="margin-left:auto;{dot 9px signal_color}"></span>
    <button style="background:transparent;border:none;color:INK_3;font-size:14px;padding:0 0 0 8px;">×</button>
  </div>
  <div style="font-family:SERIF;font-size:19px;font-weight:600;margin-bottom:6px;">{name}</div>
  <div style="font-size:13px;color:INK_2;line-height:1.6;margin-bottom:10px;">{desc 前72字}</div>
  <div style="font-size:12px;color:INK_3;margin-bottom:10px;">{meta}</div>
  <!-- 进度条 -->
  <div style="height:6px;border-radius:3px;background:#E6E0D2;overflow:hidden;">
    <div style="height:100%;width:{progress}%;background:{ui::progress_color(progress)};border-radius:3px;"></div>
  </div>
  <!-- 删除二次确认(展开态,虚线上边框) -->
  <div style="margin-top:12px;padding-top:12px;border-top:1px dashed INK_3;display:flex;align-items:center;gap:8px;">
    <span>将删除项目数据…</span>
    <button style="background:ALERT_DEEP;color:#FFF;border-radius:6px;padding:5px 11px;">确认删除</button>
    <button style="background:transparent;border:1px solid INK_3;border-radius:6px;padding:5px 11px;">取消</button>
  </div>
</div>
```
`NewCard`(wall.rs:262-277):虚线边框卡(1.6px dashed BORDER_DEEP)、`min-height:170px`、透明底、居中大「+」(26px CLAY)+「新建项目」(13px INK_3)。

---

## 4. 项目内主屏(`op.rs`)——按出现顺序

页面壳(op.rs:66-96):纵向 flex 撑满高度,底色 PAPER;从上到下 `TopBar → StageAxis → Toolbar → (LeftRail | Center)`。

### 4.1 TopBar(op.rs:100-134)
一行 flex,`padding:14px 22px;border-bottom:1px solid BORDER;`:「← 全部项目」文字按钮 → 项目信号灯(10px dot)→ 项目名(serif 17px 600)→「运营中」绿 chip → 「当前{角色简称}」阶段色 chip → 「{kind} · {周期}」INK_3 → 右侧(margin-left:auto)「北极星 · {名}」单行省略。

### 4.2 StageAxis(op.rs:136-199)
横向可滚动一行,`padding:10px 22px;border-bottom:1px solid BORDER;gap:6px;`:
- 「◎ 全部阶段 · 总览」按钮:选中时黑底白字,未选中透明+INK_2,`border:1px solid BORDER;border-radius:8px;padding:6px 12px;`。
- 5 个阶段按钮:圆点(阶段信号色)+「{序号} {阶段名}」+ 选中时「●当前」小字 + 若该阶段有活跃数 `active>0` 则右侧小红底徽标(`background:#C5654A;color:#FFF;border-radius:8px;font-size:10px;padding:0 5px;`)。选中态边框/背景取 `ui::stage_tint(kind)` 返回的三元色(bg/fg/border)。

### 4.3 Toolbar(op.rs:215-255)——两组面板切换
`padding:10px 22px;border-bottom:1px solid BORDER;` 两组按钮,中间一条 1px 竖分隔线(高 20px):
- **看板**组:进度 / Issue 看板 / 版本
- **过程件**组:工作流 / 定时任务 / 产物
每个按钮:选中黑底白字,未选中透明+INK_2,`border-radius:8px;padding:7px 14px;font-size:12.5px;`。

### 4.4 LeftRail(232px,op.rs:259-321)
- 全部阶段视图 `ActiveSessionsRail`:标题「进行中 · 待你介入」(11px INK_3);每条活跃会话是一张 `CARD_ALT` 底、`border:1px solid #DBD4C5;border-radius:8px;padding:9px 10px;margin-bottom:7px;` 的可点卡,内含标题(12.5px)+「{阶段} · {状态}」(11px INK_3)。
- 单阶段视图 `StageSessions`:标题「阶段记录」,按「创建」/「优化」两组小标题(优化组标题用 AGENT 紫色)列出同样样式的 `SessionCard`(op.rs:375-447),选中态左边框变 CLAY(1.4px)、未选中 `#DBD4C5`;每卡右上角一个「×」删除,点击后卡内展开二次确认条(同 ProjectCard 的删除条样式)。

### 4.5 总览 / 进度面板(`ProgressAll`,op.rs:2089-2364)—— Scope::All 时的默认页

四段,从上到下:

**① 项目指标 · 代码仓级**(仅当有 intrinsic 指标时渲染):`card()` padding 20/22,头部「项目指标 · 代码仓级」+ 右侧「↻ 立即采集」按钮(透明底/CLAY描边);说明小字「只当现状数 · 不点健康灯」;`grid-template-columns:repeat(2,1fr);gap:12px;` 铺 `BizMetricCard`(intrinsic 卡不显示信号灯);底部一行 mono 12px 小字「阶段完成:{各阶段} 数」。

**② 业务指标**(北极星 → 滞后 → 引领):`card()` 大容器,头「业务指标」+「↻ 同步指标文件」按钮(细边框小按钮);说明「北极星 → 滞后 → 引领 · 带健康灯」。
- 北极星卡:`BizMetricCard{is_north_star:true}` 全宽单卡,左边框 4px CLAY 加粗、名称/数值字号更大(16px/24px)。若北极星名字有但没绑定指标行,渲染一张灰色占位(`background:#F0EDE5;border:1px dashed #C8C2B4;border-left:4px solid CLAY;`),灯用 Unknown 灰,数值「—」,文案「目标未设」「无观测 · Unknown≠绿」。
- 滞后性指标段:mono 10.5px 600 字距 0.08em CLAY 小标题「滞后性指标 · 结果型 · 看趋势不追本周」+ 两列网格 `BizMetricCard`。
- 引领性指标段:同上样式,标题「引领性指标 · 定本周驱动项…」,卡片额外带「本周目标 + 达成 ●/○」。
- 底部折叠区 `ArchivedMetrics`(见 §4.6)。

**③ buddy 情况**(非卡片单行,op.rs:2320-2339):左边框 3px CLAY、底色 `CARD_ALT`、`border-radius:0 8px 8px 0;padding:11px 14px;`,一行 flex-wrap 拼接多个 span:mono 标签「buddy 情况」· 「●{阶段}阶段」· (若有)「N 条 Issue 评审中待你 merge」· (若有)「N 个指标本周未记」·「本周完成 N / 开放 N」(数字 mono 600 CLAY 色)。

**④ ▾配置**(默认收起):`card()` 整体,头部是一个可点按钮(占满宽度):折叠箭头(▾/▴,mono 14px)+「配置」(serif 600 15px)+ 说明「收进次级」+ 右侧「展开▸/收起▾」。展开后 `border-top:1px solid BORDER;padding:14px 18px;` 内含 `EditProjectCard`(项目名/类型/描述/对标/成功标准/周期编辑,同款 label()+input() 表单)、`WorkspaceConfig`(真执行工作目录路径 + 允许运行命令勾选)、`AttachRepoCard`(未接仓时显示 owner/repo 输入 + 「接入」按钮)。

### 4.6 指标卡两种形态

**`MetricCard`**(阶段面板用,op.rs:2577-2673):`card()` padding 16/18。头行:信号灯 dot 9px + 名称(13px 500)+ 右侧(margin-left:auto)采集来源徽 or 「手填 · 未接入度量源」灰底 chip。数值行:mono 22px 600 数值 + 「目标 {x}」(12px INK_3)。下方 `Spark`(120×34 迷你折线,面积透明度0.13)。有 def 文字则显示定义(11.5px INK_3)。非停用态底部是 `RecordInline`(输入框+「记录」按钮);停用态整卡 `opacity:0.55`,底部一行「已停用…冻结值…」+「恢复」按钮。

**`BizMetricCard`**(总览用,op.rs:2678-2860,更完整):`card()` padding 14/16,`flex-direction:column;gap:10px;`。
- 头:信号灯(intrinsic 不显示)+ serif 600 名称(北极星 16px/其余 14px)+ 右侧采集标签/「手填」chip。
- 值行:serif 700 数值(北极星 24px/其余 22px,无观测显「—」)+「目标 {x}」或「目标未设」(INK_4);引领指标额外显示「本周目标 {x}」+ 达成圆点(●绿/○红/—灰,`opacity:.55`)。
- delta 行:「vs 上周」+ mono 600 箭头数值(↑绿#5F7355 / ↓红#B0503A / →灰),无观测再加「无观测」灰字。
- **`WeeklyTrendChart`**(op.rs:2393-2499,卡片级折线,不是迷你 spark):320×128 SVG,浅色 `#FBF9F4` 绘图区背景 + `#E8E2D6` 网格虚线 + y 轴刻度文字(mono 10px)+ 面积(透明度0.12)+ 折线(stroke-width 2.2 圆头)+ 每个数据点白心圆圈 + 点上方数值标签(mono 10px 600,信号色)+ 点下方 x 轴日期标签。无数据时整块 120px 高、`border:1px dashed #E2DCCF;` 居中显示「尚无观测 · 折线空」。
- 底部采集链说明(仅北极星卡或无观测卡显示):「采集链: {connector名} → {cron有无tick} → {chain_tail}」,11px INK_3。
- 定义文字(可选)。
- manual 采集类型且未停用 → 底部嵌 `RecordInline`。

**「已停用」折叠区** `ArchivedMetrics`(op.rs:2544-2575):`border-top:1px dashed #ECE6DA;padding-top:10px;` + 一个纯文字按钮「▸/▾ 已停用 (N)」,展开后两列网格铺灰化的 `MetricCard`,一条不渲染时整段不出现。

### 4.7 单阶段进度页
- 原型阶段(`PrototypeProgress`,op.rs:3024-3102):特殊——上方一行(标题+阶段chip+「Open Design · 首页」+「重新发现」按钮),中间 `flex:1` 嵌一个 `<iframe>`(圆角 10px 边框)展示本机 Open Design 首页,没接到时显示占位卡;底部可折叠「▸/▾ 完成清单与交棒」展开 `StageDetailCard`。
- 其余阶段(`ProgressStageLegacy`,op.rs:3104-3197):标题行 + 「立即采集」按钮;两列网格铺该阶段 `MetricCard`(空态显示提示卡);`ArchivedMetrics`;`card()` 内「进度趋势(手动维护的计划数据)」大折线(520×74)+ 数字输入「更新进度」;最后接 `StageDetailCard`。

**`StageDetailCard`**(方法论卡,op.rs:2862-2995):`card()` padding 20/22 margin-top 16。头行:角色名(serif 15px)+「方法论 · {名}」chip + 右侧(margin-left:auto)大字「{seek}」(serif 15px 阶段色)+ 节奏小字。「核心问题」一句(serif 14.5px)。「方法循环」一排 chip 用「→」连接、末尾「↺」符号(阶段色)。两列网格:「默认视图/引领焦点」vs「AI 编队」(agent 名高亮阶段色)。深色反模式条(`background:#23211C;`,粉色标签「反模式」+ 浅字正文)。**交棒清单 DoD**:阶段色左边框 3px + 阶段浅底,每条是 16×16 圆角方框(勾选=阶段色底白勾,未勾=透明+浅描边)+ 文字,点击直接 toggle;当前阶段才显示交棒按钮(阶段色底,未勾满时旁边红字警告「未勾满也可交棒 · 将记「带险交棒」」)。

### 4.8 Issue 看板(`IssuesPanel`,op.rs:759-1309)

**创建条**(`card()` padding 12/16,flex-wrap):标题输入框(flex:1)+ 5 个阶段圆角胶囊 chip(选中 CLAY 底白字)+ 技能下拉 `<select>` + 「＋ 创建 Issue」按钮(CLAY 底白字)+ 右侧(有远端仓时)「↻ 从仓同步 Issue」细边框按钮。

**六列看板**:`display:flex;gap:12px;`,每列 `flex:1;min-width:190px;`,列名「{label} · {count}」(11.5px INK_3):
```
待办池 → 待办 → 进行中 → 评审中 → 已完成 → 阻塞
```
每张卡(`card()` padding 10/12,`border-left:3px solid {状态色或聚焦时CLAY}`):
1. 顶行 mono 11px INK_3:「#{编号} · {阶段}」+ (聚焦)「当前会话」CLAY 描边小 chip + (恢复中)灰描边「恢复中…」chip。
2. (可选)开放 MR 提示行、远端 Issue 链接行(「远端 #N ↗」)、PR 链接行(「PR #N ↗」CLAY 色)。
3. 标题(13px INK,可点击打开详情弹层)。
4. 优先级文字(11px INK_2)。
5. 指派 `<select>`(11.5px,细边框圆角5px)。
6. 若阻塞:浅橙底提示条「⛔ {原因}」+ 两个「解除→待办/进行中」文字按钮(CLAY色)。
   若正在填阻塞原因:一个 11.5px 输入框 +「确认阻塞」(红字)/「取消」。
   否则一行操作区(`gap:12px`,全部是无边框透明按钮,颜色区分语义):
   - 运行中 →「⬇ 终止」(ALERT_DEEP 加粗)
   - 可运行 →「▶ 跑」(CLAY,若同项目已有活在跑则「▶ 跑(排队中)」灰色 disabled)
   - 可续聊 →「续聊」(CLAY)
   - 评审中且有 PR →「⬇ merge PR #N」(CLAY,合入中变灰「正在合入…」)
   - 有下一状态 →「→ {下一状态名}」(CLAY)
   - 可阻塞状态 →「⛔ 阻塞」(INK_3)

**Issue 详情弹层**(`IssueDetailOverlay`,op.rs:1318-1683):绝对定位覆盖看板中心区(`position:absolute;inset:0;background:rgba(35,33,28,.38);z-index:60;`,点击背景关闭),内容卡 `card()` `width:720px;max-height:82vh;overflow-y:auto;padding:18px 22px;`:
- 头行:mono 11.5px「#N · 阶段 · 状态」+ 右侧「✕」关闭。
- 标题(16px)、指派+优先级、技能说明行、(有PR)PR行、(阻塞)红色提示条、描述正文。
- 「运行史(N)」:每条运行是 `border:1px solid BORDER;border-radius:8px;padding:8px 11px;`,状态点(●绿成功/●红失败)+ mono 细节;失败有红色错误文字;变更列表按文件展示 `+add`(绿)/`-del`(红)。
- 「产物登记(N)」:mono 细行,路径 + commit + 字节数。
- 操作区(同看板卡片按钮语义,按钮更大 `padding:7px 16px`):「▶ 跑」/「⬇ 终止」/「续聊」+「转成新活」/「⬇ merge PR(验收)」或「✓ 确认完成(人裁)」+「↩ 打回」/「⚗ 蒸馏为技能」;已记账则显示灰字「已记账(同一件活绝不记两次)」。
- 蒸馏表单(点「⚗ 蒸馏为技能」展开):名称/描述输入框 + 正文 textarea(mono 11.5px)+「确认蒸馏」/「取消」。

### 4.9 工作流面板 / 嵌入终端(`WorkflowPanel`→`WorkflowStage`,op.rs:3201-3580)

- 未选中具体阶段(Scope::All)时显示「从 Hub 导入」引导语 + `HubOverviewStrip`:三列网格,每张 `card()` 卡是一个组件库入口(技能/智能体/工作流),含信号点+名称+计数、类型说明、简介、chip 标签列表、「浏览并导入 →」链接按钮。
- 选中阶段后 `WorkflowStage`:
  1. `RunBanner`(仅当有真实 run 时,op.rs:3293-3343):`CARD_ALT` 底卡,`border:1px solid #DBD4C5;border-radius:10px;padding:14px 16px;`,头行「{工作流名} {状态文字}」;`PhaseTrack` 步骤条(见下);底部虚线上边框展示涉及的 agent(紫色chip「◆名」)/skill(米色chip「🧩名」)。
  2. **`PhaseTrack`**(op.rs:3349-3405):一排编号圆形徽章(24×24,2px 描边),用横线连接——完成=白底绿框绿勾✓、失败(当前步)=白底红框红叉✕、进行中=CLAY实心白字数字、未开始=白底浅灰框灰数字;每个徽章下方是阶段名(10px)。
  3. 方法循环预览卡(仅无终端会话时显示):`card()`,「{工作流名}」+「方法循环:A → B → C」+「验收:{目标} · loop ≤3 迭代」+ (有会话)「↑ 沉淀为静态」按钮。
  4. `RunOutputs`「产出」卡(有 agent 消息时才出现):`card()`,每条产出是「{序号}. {阶段名}」(CLAY 11px 加粗)+ 正文(12.5px,pre-wrap),条目间虚线分隔。
  5. **`Chat`**(非 PTY 会话,op.rs:3620-3692):`card()` padding 16/18,头「{标题}」+ 状态 chip;消息列表 `max-height:420px` 可滚动,Agent 消息靠左白底、Builder 消息靠右黑底反色(气泡 `border-radius:10px;padding:8px 12px;max-width:72%;`,发送者标签 10px AGENT紫);底部输入区 `textarea` + 「发送」按钮(CLAY)。
  6. **嵌入终端 `TerminalWidget`**(PTY 会话,xterm.js,op.rs:4015-4148):外框 `border:1px solid BORDER;border-radius:8px;overflow:hidden;flex:1;display:flex;flex-direction:column;`;顶部深色标题条 `background:#1e1e2e;color:#cdd6f4;font-family:mono;font-size:11px;padding:4px 10px;` 内左「● in-app terminal」(opacity .7)、右「claude interactive session」(opacity .4,margin-left:auto);下方 `flex:1;background:#1e1e2e;` 是 xterm.js 挂载点。暗色(`#1e1e2e`/`#cdd6f4`)是全屏唯一暗色区域,其余全暖纸浅色系。多会话时未聚焦终端用 `position:fixed;left:-10000px;opacity:0;` 离屏保活,不用 `display:none`。

### 4.10 产物面板(`ArtifactPanel`,op.rs:512-594)
`card()` 顶部状态条(工作区路径/未配置提示 +「读取登记」+「重新采集」按钮)+ 逐行卡片列表:kind 色块 chip(文档蓝#4F7E86/代码CLAY/测试绿#6E8C5A/脚本橙#CC8B3C/配置灰#8A8275)+ mono 路径(flex:1 省略号)+ 阶段/版本数/run产出标记 + 字节数 + commit + 时间(均 mono 11px INK_3)。

### 4.11 版本面板(`VersionPanel`,op.rs:600-662)
同 ArtifactPanel 的状态条模式(「刷新提交记录」代替读取/采集),逐行是真实 git log:mono short_hash + subject(flex:1 省略)+ author + mono 日期。

### 4.12 定时面板(`RoutineAll`/`RoutineStage`,op.rs:3696-3787)
`RoutineAll` 是单张 `card()` 按阶段列出信号点+阶段名+「节奏 · 盯 N 项」的横排列表行(虚线分隔);`RoutineStage` 是监测项 chip 云 + 「观测流」时间轴列表(时间 mono + 按 FeedLevel 上色的文字,红/黄/INK_3)。

---

## 5. Hub 屏(SkillHub / AgentHub / CronHub / ConnectorHub / SettingsHub)

四个组件库 Hub 共用同一套页面骨架:
```
<div style="padding:22px 26px;overflow-y:auto;">
  <span style="mono 11px 字距.06em INK_3;">SKILLHUB / AGENTHUB / CRONHUB / CONNECTORS</span>  <!-- 全大写英文小标 -->
  <div>标题(serif 22px 600)+「N 技能/智能体/…」灰字计数,右侧「+ 新建…」按钮(透明底CLAY描边)</div>
  [筛选 chip 行]   <!-- Skill/Agent 才有,按五阶段角色 -->
  [创建表单]        <!-- 点「+」展开的 card() 内嵌表单,name/desc/category/content 四个 label()+input() -->
  [列表/网格]
</div>
```

### 5.1 SkillHub / AgentHub(卡片网格)
- 网格:`grid-template-columns:repeat(auto-fill,minmax(300px,1fr))`(Skill)/ `minmax(340px,1fr)`(Agent),`gap:14px`,宽窗口自动加列而非固定 3 列。
- 筛选 chip 行:「全部」+ 五阶段(阶段色)+「全阶段通用/不属任何阶段/未归类」(Skill 五枚全有;Agent 只有「未归类」)。选中 CLAY 底白字,未选中 `#EFE9DA` 底 INK_2 字,圆角 chip。
- **SkillCard**(skill_hub.rs:486-632):身份行(mono 13px 名称 + 归属「◇项目名」chip + 规范违规 chip + 右侧成熟度 chip)→ 五角色归属 chip 行(阶段色/CLAY全阶段/灰不属任何/黄未归类)→ 一句话描述 → mono 引用数统计行 → 出处行(分类·来源标签·改编自·蒸馏徽记绿色chip)→ 正文首句预览(mono 11px 省略)。点击身份行展开:`SkillFileBrowser` + 规范细则 + 「被这些工作流使用」chip 列表 + 「编辑 →」。
- **`SkillFileBrowser`**(skill_hub.rs:295-400,「文件树+预览」双栏包浏览器,所有技能统一走这个模板,哪怕只有 SKILL.md 一个文件):`display:flex;border:1px solid BORDER;border-radius:8px;overflow:hidden;`——左栏 200px `CARD_ALT` 底可滚动文件树(mono 12px,选中态 CARD 底 CLAY 字);右栏 `flex:1` 上方 mono 文件名条(虚线下边框)+ 下方 markdown 渲染或等宽 `<pre>` 原文(`max-height:360px` 可滚动)。
- **AgentCard**(agent_hub.rs:175-...):身份行是 36×36 圆角方块头像(agent紫底白字首字母)+ 名称/角色两行 + 归属/来源/成熟度三个 chip;下方战绩行「N 次运行 · 成功率 X%(或「—(无运行证据)」)· 被 N 个工作流使用」;模型 chip + 装备技能 chip 云;展开显示 markdown 常驻指令 + 使用方列表 + 编辑按钮。

### 5.2 CronHub(表格式)
- 表头/行是 `grid-template-columns:1.3fr .9fr .9fr .8fr .8fr 1.4fr;gap:10px;`:任务/目标、频率、项目、上次/下次、状态、操作。表头一行 11px INK_3、底边框实线;每行虚线下边框、`padding:10px 16px;`。
- 任务名前带模式图标(🔄运行工作流/⚙运行技能/💬运行Prompt),Prompt 模式下目标是可点省略文字「点击展开全文」。
- 状态 chip 颜色随 `CronStatus`(Failed=红 ALERT_DEEP / Running=CLAY / Paused=INK_3 / Normal=INK_2)。
- 操作列:「▶ 立即执行」(可执行=`btn_primary()` 缩小版;不可执行=灰色描边 disabled 带 title 说明)+「⏸ 暂停/▶ 恢复」文字按钮。

### 5.3 ConnectorHub(3列卡片网格)
- `grid-template-columns:repeat(3,1fr);gap:14px;`。每卡:32×32 圆角方块首字母图标(浅底描边,非 agent 紫)+ 名称/类型 + 右侧状态 chip(Connected=绿#E5EBDD/#4D6B3C,Error=红#F3DFD8/ALERT_DEEP,其余=中性 #EFE9DA)。下行:作用范围(省略号)+ 最近同步时间 + 「立即同步」按钮(可同步)或灰字「登记项 · 无真实探针」。

### 5.4 SettingsHub(单卡表单)
`max-width:640px`,单张 `card()`「模型与额度」。只读态是若干 `Row`(label 左 INK_3 / value 右 INK_2,`border-bottom:1px solid #EFEADf`)+「修改」按钮。编辑态是 input(claude 二进制路径)+ number input(预算)+ 两个自绘 checkbox(☑/☐ 文字模拟,不用原生 checkbox)+ 触发 bypassPermissions 的红字警告 + 「保存」/「取消」。

### 5.5 组件详情面板(`component_detail.rs`,project_rail 点击后原地展开)
统一骨架:`padding:22px 26px;` 顶部「← 返回项目」+ mono 小标题「本项目组件 · 完整详情」,下方是单张 `card()` `padding:22px 26px;max-width:680-800px;`,按类型分四种正文(Skill 复用 SkillFileBrowser;Agent 是头像+markdown指令;Workflow 是流程图/文档双视图 + 涉及智能体/技能 chip;Cron 是有效性统计;Connector 是简单三行)。所有类型头部都带「◇ 项目名」归属 chip 和「⤓ 引入本项目 · {项目名}」按钮(仅全局共享行且有活跃项目时出现)。

---

## 6. 创建流两卡(`create.rs`)

容器:`max-width:640px;margin:0 auto;padding:36px 24px 120px;`,纵向 `gap:12px`。顶部一行:标题「新建项目」(serif 17px)+「← 返回项目墙」文字按钮(永远可退出)。其下常驻 `ActionsBanner`(后台动作进度条,见 §6.3)。

### 6.1 Card::Repo「仓从哪来?」(create.rs:278-675)
1. 标题(serif 22px)+ 说明句。
2. `platform_selector`:GitHub / CodeHub 两个圆角胶囊 chip(选中 1.5px CLAY 边框实心、未选 1px 浅描边)。
3. (CodeHub 时)`codehub_host_selector`:「绿区 green / 内源 open / 黄区 yellow」三个同款胶囊 chip + 下方说明小字。
4. `chip_question("起点", ["新建仓","接入已有仓"])`:同款胶囊 chip 二选一。
5. `card()` 主体区,按「新建/接入」+「github/codehub」四种组合切换字段:
   - 新建:仓库名 input +（github 无 namespace;codehub 多一个 namespace input)+ 可见性 chip_question(Private/Public)。
   - 接入:「选一个仓」标签 + 「↻ 刷新列表」按钮 + `RepoCombobox`(可搜索下拉,列表来自真实 `gh repo list`/codehub API)+ 选中后回显完整 metadata 块(描述/可见性/默认分支/最近推送)。
6. 底部一行:左侧(若有)探活提示文字(见 §6.3)、右侧校验提示 + 「下一步 →」主按钮(`btn_primary()`,disabled 时 opacity .45)。

### 6.2 Card::Intent「你想做什么?」(create.rs:885-1191)
- 标题 + 说明句(三态文案:只读正本/识别中/默认)。
- 若探测到远端已是 Buddy 项目:绿色徽记条「已是 Buddy 项目 · 正本只读」(`background:#E8F0E4;border:1px solid #C5D6BE;color:#5F7355;`),此时下方所有字段变只读态(浅灰底 `#F5F1E8`)。
- 若探测失败:琥珀色提示句(`color:#B5862F`)。
- `card()` 表单:两列网格(项目名称 * / 项目类型下拉)→ 「你想做什么」textarea → 两列网格(最像的对标 / 三个月后怎样算成)。
- 底部右对齐主按钮,三态文案:「确认 · 建立项目」(CLAY) / 「识别中…」/「建立中…」(均灰 `#B89A8E` disabled)。
- 底部说明句(建仓/connector/cron/三件套自动化的提示)。

### 6.3 「探活三区」——远端探测(`RemoteProjectProbe`)在两张卡上的三处呈现
1. **RepoCard 底部提示**(`probe_hint`,行 399-410):四态文案——Probing「正在识别是否已是 Buddy 项目…」/ Present「已识别:仓里有 .bw/project.toml(后来者 · 下一步只读预填)」/ Absent「仓里尚无 .bw/project.toml(首到者 · 下一步手填意图)」/ Failed「正本探测失败,下一步仍可手填」。灰色小字,`margin-right:auto`。
2. **IntentCard 只读徽记**(Present 态,行 1077-1082):绿色 pill 徽记「已是 Buddy 项目 · 正本只读」,字段联动锁定。
3. **IntentCard 失败提示**(Failed 态,行 1083-1087):琥珀色句子,允许手填但注明「以 clone 后的仓文件为准」。
四态背后是同一个 `RemoteProjectProbe` 枚举(Idle/Probing/Present/Absent/Failed),UI 上没有「探活区」独立卡片,而是拆成上述三处文案+一处字段锁定态,合起来构成「探活」体验。

### 6.4 `ActionsBanner`(后台动作条,create.rs:217-274)
纵向 `gap:4px` 的行列表,自带 400ms 心跳刷新「已 N 秒」。三态:
- 进行中:CLAY 6px 圆点 + 「正在{名} … 已 N 秒」(INK_2)。
- 成功:绿字「✓ {名}」(#5F7355)。
- 失败:红字「✕ {名} · {人话原因}」(#B0503A),原始报错放 `title` 悬浮。
秒级内完成的动作完全不显示(阈值门槛,防止闪烁噪音)。

---

## 7. 值得原样保留的好东西

1. **`theme::chip/card/dot/btn_primary/input/label` 五个原子函数**(`theme.rs:53-89`)——一套极简 token-to-CSS-string 系统,颜色/圆角/间距全站统一,HTML 原型应直接抄成同名 CSS class 或 CSS 变量,不要另起一套。
2. **`PhaseTrack` 步骤条**(`op.rs:3349-3405`)——编号圆形徽章 + 连接线,四态配色(完成绿勾/进行中实心数字/失败红叉/未开始灰数字)清晰讲清「运行到哪一步」,比进度条更适合工作流可视化,直接照抄。
3. **`WeeklyTrendChart` 卡片级折线**(`op.rs:2393-2499`)——带 y 轴网格虚线、x 轴日期标签、数据点数值气泡的 SVG 折线,比普通迷你 sparkline 信息量高很多,业务指标卡的标准配置。
4. **`SkillFileBrowser` 文件树+预览双栏**(`skill_hub.rs:295-400`)——即使只有一个 SKILL.md 也套同一个「文件夹」外壳(200px 树 + 右侧预览),模板一致性优先于「单文件不用画树」的直觉简化,值得沿用这个设计决定本身。
5. **健康三态配色纪律**:「绿色隐身,只有红黄出声」——ProjectCard/HealthOverviewBar 里绿色信号只折叠计数不单独强调,红/黄/Unknown 才用醒目色块,是贯穿全站的视觉哲学,HTML 原型的配色规则必须继承这一条,不能画成四色等权重的仪表盘。
6. **Issue 看板卡片的按钮语义分层**(`op.rs:1160-1297`):同一按钮位置按状态互斥切换文案+颜色(▶跑/⬇终止/续聊/⬇merge/→下一步/⛔阻塞),而不是堆砌一排常驻按钮再灰置不可用项——减少视觉噪音,值得直接搬。
7. **`ActionsBanner` 的三态 + 阈值门槛**(`create.rs:162-207`)——秒级完成的后台动作不显示、超阈值才显「已N秒」+ linger 淡出,是一个成熟的「诚实但不聒噪」的异步反馈模式,通用性强,原型里任何后台调用都可以复用这一套。
8. **嵌入终端的暗色卡片**(`op.rs:4134-4147`)——`#1e1e2e`/`#cdd6f4` 暗色是全站唯一暗色区域,专门用来标识「这里是真实 shell,不是网页」,这种局部反色对比手法(而不是给整个应用做深色模式)值得保留。
9. **停用指标的「冻结但不删」交互**(`op.rs:2536-2673`)——整卡降低不透明度 + 明确文案「上方信号为停用时刻的冻结值,历史观测一条未删」+「恢复」按钮,而不是真删除,体现 append-only 的产品哲学,原型如果要做「归档/停用」类交互应直接复用这个模式。
