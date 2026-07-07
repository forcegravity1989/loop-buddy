# Builders' Workbench · Rust 跨平台 UI 选型评估

> 目标:把现有 React 风格的复杂前端原型(`Builders工作台-项目管理向导.dc.html`)重写成**原生桌面应用(Rust)**。**桌面唯一(macOS + Windows);Web 是「以后也许」,绝不驱动任何 MVP 决策。** 真实诉求:一个**比 Electron 更快更轻**的桌面应用(复用系统 WebView,不打包 Chromium)。
>
> 调研口径:2026-06-26 联网核实(crates.io API / GitHub Releases API / 官方文档原文)。WebSearch/WebFetch 当时后端不可用,改用 `curl` 直取权威端点的原始数据,版本号与能力均为实测,非凭记忆。

---

## 0. 结论先行(TL;DR)

- **首选(确认):Dioxus 0.7 桌面(desktop = `wry` webview),RSX + Tailwind。** 一句话理由:**系统 WebView 既让它在冷启动/内存/包体上打败 Electron(复用 OS 浏览器内核,不打包 Chromium,见 §1.5),又是真 CSS 引擎,让你 1619 条内联 CSS、暖纸 clay 色板、中文衬线混排、SVG sparkline 近乎平移;同时 RSX 是 Rust 里的 JSX,贴合 dc-runtime 的 React 心智,迁移成本最低。**(「一套 RSX 同出桌面+Web」在桌面唯一后降级为**未来红利**,不再是首选理由 —— 决策改由「迁移贴合度 + DX + 无 IPC 边界」支撑。)
- **次选:Tauri v2 + Leptos(或 Yew)。** 当 Dioxus 的某个坑(见 §6)挡路时,退到「Tauri 管壳与签名分发 + Rust 的 Web 框架(Leptos/Yew)写前端」。前端仍是 Rust、仍跑在 WebView 里,设计还原度一样满分;代价是桌面/Web 不像 Dioxus 那样天然同构,且多一层 Tauri IPC。
- **不选(对本原型):Slint、egui/eframe。** 它们是**非 HTML/CSS 的自绘 UI**,要把暖纸色板、精细圆角阴影、中文混排、文档式富排版逐像素重建为另一套 DSL(Slint 的 `.slint`)或 immediate-mode 代码(egui),迁移成本极高且设计上限受限——与「设计感极强 + 文档式富 UI」正面冲突。
- **对「Tauri 前端非 Rust」这一张力的裁决:** Tauri **官方定位就是「前端 JS、逻辑 Rust」**,违背你「语言用 Rust」的硬约束。**但这不是非 Tauri 不可**——把渲染层换成 **Dioxus(RSX 即 Rust)** 或在 Tauri 壳里塞 **Leptos/Yew(Rust→WASM)**,即可让**前端也是 Rust**,同时保住 WebView 的设计还原度。故裁决为:**不接受"裸 Tauri + JS 前端",接受"Rust 前端(Dioxus / Leptos / Yew)+ WebView 内核"。**

---

## 1. 这个原型到底有多"重"(决定一切的事实)

先量化本原型的 UI 特征,因为它直接决定哪种渲染模型能活下来(数据来自对 `Builders工作台-项目管理向导.dc.html` 与 `support.js` 的实测扫描):

| 维度 | 实测 | 对选型的含义 |
|---|---|---|
| 渲染内核 | `dc-runtime`(`support.js`)→ **React.createElement**;模板用 `<x-dc>` + `{{ }}` 插值(**750 处**)+ `<sc-if>`/`<sc-for>` 指令 | 心智模型 = **声明式模板 + JSX**。最贴近的是 **RSX(Dioxus)/ JSX**,而非 immediate-mode、也非 `.slint` DSL |
| 样式 | **1619 条内联 `style=`,0 个 CSS class**;`border-radius` 覆盖 3–12px 全谱;`box-shadow` 11 处 | 这是一套**手调到像素级的 CSS 设计系统**。只有「真 CSS 引擎」能低成本平移;自绘 UI 要逐条重写 |
| 色板 | 暖纸/clay 自定义色:陶土 `#C5654A`/`#B0503A`、暖墨 `#23211C`、纸 `#FBFAF6`、鼠尾草绿 `#5F7355`、金 `#B5862F` 等数十色 | 任意颜色,所有方案都能做;但**搭配精细渐变/`color-mix`/半透明叠层**时,CSS 引擎最省事 |
| 字体 | **Noto Sans SC(431)+ Noto Serif SC(46)+ JetBrains Mono(342)** 混排,中文优先 | 需要**任意自定义字体 + 高质量中文(CJK)排版**。Web 内核天然搞定;自绘 UI 要自己喂字体、处理 CJK 断行与字重 |
| 动态图形 | **SVG sparkline**(JS 现算 `path`/`polygon` 折线与面积,指标趋势图 + 进度趋势大图);`color-mix`/渐变流动效果;streaming shimmer 动画 | SVG + CSS 动画在 Web 内核里是原生能力;自绘要用 `epaint`/`femtovg` 重画矢量 |
| 体量 | HTML ≈ 3283 行;`support.js` ≈ 1595 行 | 是「文档式富 UI / dashboard」,不是游戏或工具条——**正是 immediate-mode(egui)最不擅长、Web 内核最擅长的形态** |

**一句话:这是一套"用内联 CSS 手工调出来的、文档式、中文优先、带实时矢量图"的设计稿。能不能低成本迁移,等价于问"目标栈是不是真 CSS 引擎"。**

---

## 1.5 为什么 WebView 比 Electron 更适合(Rust 的真实理由)

> 用户选 Rust 不是为了「用 Rust」,而是要一个**比 Electron 更快更轻**的桌面应用。关键洞察:**Rust + 系统 WebView 已经满足这个诉求** —— 复用 OS 自带浏览器内核,不像 Electron 每个应用各自打包整个 Chromium。下表为**量级估计,非本文实测**。

| 维度 | Electron | Rust + 系统 WebView | 倍率 / 说明 |
|---|---|---|---|
| **冷启动**(到首次可交互) | ~600ms–2.0s(起 Chromium+Node+V8+渲染进程树) | ~150–600ms(原生二进制 attach 已驻留的 OS WebView) | **~2–4× 更快**,最被用户感知。Win10 首次 WebView2 预热会偏高 |
| **空闲内存**(单窗口) | ~120–300MB+(各自一份 Chromium,跨应用不共享) | ~40–120MB(OS WebView 共享 + Rust 堆小,少一个 Node/V8) | **~2–3× 更轻**。全 11 panel 同挂会缩小差距 → 懒挂面板 |
| **安装/下载包体** | ~85–200MB 装机(~50–90MB 压缩;Chromium+Node+ICU 全打包) | mac ~8–25MB(系统 WKWebView,零运行时);Win 类同(WebView2:Win11 自带 / Win10 Evergreen 引导) | **~5–15× 更小**,最戏剧的差距。CJK 字体两边都 +~10–20MB |
| **构建/CI 复杂度** | npm/node + electron-builder + node-gyp 原生重建;模板海量 | cargo + `dx bundle`(复用 Tauri 打包);签名/公证与 Electron **同等**,无 node-gyp | **大致打平**。签名是真成本,但平台强制、非栈强制 |
| **安全更新面** | **你背 Chromium CVE 跑步机**:每个 Chromium/V8/Node 安全版都逼你重发,否则用户跑漏洞浏览器 | OS 厂商补 WKWebView / WebView2(Evergreen 自动更新),**浏览器内核 CVE 不在你盘里** | **Rust 持久大胜** —— 单人开发者最大经常性安全负担直接消失 |

> **净结论**:WebView 已拿走「打败 Electron」**~90%** 的收益。Slint/egui 无-WebView 自绘只能再挤最后 ~10%(再省点内存、再压点冷启动),代价是把整套 CSS 设计系统逐像素重画 —— 对本原型不划算(§4 理由五)。唯一 Electron 反占优之处:它自带 Chromium,Win10 缺 WebView2 时不白屏;对策见 §7.3 的 Evergreen 引导。若「打败 Electron」要成为对外承诺,可花 1–2 天做个真 benchmark spike(量 Phase-0 切片 vs 一个平凡 Electron 对照),而非把上表当事实发布。

---

## 2. 候选栈最新状态(2026-06,实测版本号)

> 版本/下载量取自 crates.io API(`updated_at` 多为 2026-05 ~ 06,数据为实时);能力取自各项目官方 README / 发布博客原文。

| 栈 | 最新稳定版(2026-06) | 渲染内核 | 前端语言 | Web(WASM) | 近 90 天下载 | 备注 |
|---|---|---|---|---|---|---|
| **Tauri** | **2.11.3**(2026-06-17);`wry` 0.55.1 / `tao` 0.35.3 | 系统 WebView(macOS=WKWebView,Win=WebView2) | **JS/任意 web 框架**(非 Rust) | 是(就是网页本身) | tauri ~7.19M;wry ~7.67M | 桌面壳 + Rust 后端;前端不限框架 |
| **Dioxus** | **0.7.9**(2026-05);`0.8.0-alpha.0`(2026-05-19,推进 Blitz) | desktop=`wry` WebView;web=WASM→DOM;native=Blitz(WGPU,实验) | **Rust(RSX,JSX 同构)** | 是(成熟,CSR/SSR/hydration) | dioxus ~0.52M | 一套 RSX 出 Web/桌面/移动;Subsecond 热补丁;Tailwind 内建 |
| **Slint** | **1.17.0**(2026-06-24) | 自有 GPU 渲染(Skia/FemtoVG 等),UI 编译为机器码 | **`.slint` DSL**(+ Rust 业务逻辑) | 是(WASM,canvas) | slint ~0.30M | 桌面/嵌入式/移动;Figma→Slint 插件;非 HTML/CSS |
| **egui / eframe** | **0.35.0**(2026-06-25) | immediate-mode 自绘(`epaint`→wgpu/glow) | **Rust** | 是(WASM+WebGL) | egui ~4.32M / eframe ~3.62M | 工具/游戏 UI 之王;样式"尚不如 CSS 强";不适合文档式富 UI |
| **Leptos + Tauri** | Leptos **0.8.20**(2026-06-25)+ Tauri 2.11.3 | Tauri WebView | **Rust→WASM** | 是(细粒度反应式,SSR 强) | leptos ~0.97M | Rust 前端跑在 Tauri 壳里;桌面/Web 非天然同构 |
| **Yew + Tauri** | Yew **0.23.0**(2026-03)+ Tauri 2.11.3 | Tauri WebView | **Rust→WASM** | 是(VDOM,类 React) | yew ~0.32M | 同上;Yew 迭代节奏明显慢于 Leptos/Dioxus |

补充支撑事实(实测):
- **Dioxus Desktop** README 原文:*"Dioxus VirtualDom running on a native thread — **Full HTML/CSS support via `wry` and `tao`**"*。即**桌面端用的就是和 Tauri 同款 WKWebView/WebView2,CSS 还原度 = 浏览器级**,而 UI 源码是 Rust RSX。
- **Dioxus Native / Blitz**(无 webview 的 WGPU 渲染器,基于 Servo 的 **Stylo** CSS 引擎 + **Taffy** 布局 + **Parley** 文本):README 明说 *"there are also still many bugs and missing features … **we would not yet recommend building apps with it**"*。→ **2026 年仍非生产可用**;本原型的桌面端应走 **webview 渲染器**,而非 Native。
- **Tauri v2** 官网原文:*"Write your frontend in **JavaScript**, application logic in **Rust**"*——这正是"前端非 Rust"的官方定位(§7 张力裁决的核心)。Tauri 也提供 **Sidecar(Embedding External Binaries)**,可在 JS 前端的壳里挂一个 Rust 核心二进制。
- **egui** README 原文:样式 *"is not yet as powerful as say CSS"*;布局每帧重算(大滚动区有成本);CJK 需手动加载字体。
- **Slint** 许可:**专有桌面/移动/Web 应用可用 Royalty-free 许可免费**,开源用 GPLv3,或买 Commercial——**做闭源商业桌面产品无需付费**(此前常见误解,特此更正)。
- Tauri 2.11.3 的 cargo-audit 显示其 **Linux** 侧仍依赖已停维护的 GTK3(`atk`,RUSTSEC-2024-0413)——但**仅影响 Linux**;macOS/Windows 用系统 WebView,不受影响(本项目桌面优先 Mac+Win,可忽略)。

---

## 3. 对比表:维度 × 方案

评分:★★★★★ 最佳 → ★ 最差。**加粗维度**是本原型的命门。

| 维度 | Tauri v2 (+JS) | **Dioxus 0.7** | Slint 1.17 | egui 0.35 | Leptos+Tauri | Yew+Tauri |
|---|---|---|---|---|---|---|
| **前端语言是 Rust** | ✗ JS/TS | ★★★★★ RSX | ★★★☆ `.slint`+Rust | ★★★★★ | ★★★★★ | ★★★★★ |
| **设计还原度(暖纸/精细 CSS)** | ★★★★★ WebView | ★★★★★ WebView | ★★★ 自绘需重建 | ★★ 受限于 set_style | ★★★★★ | ★★★★★ |
| **中文混排(Noto Serif/Sans SC)** | ★★★★★ | ★★★★★ | ★★★★ 需配字体 | ★★★ 手动喂字体 | ★★★★★ | ★★★★★ |
| **SVG / sparkline / 矢量图** | ★★★★★ 原生 SVG | ★★★★★ 原生 SVG | ★★★★ 内建矢量+Path | ★★★ epaint 手画 | ★★★★★ | ★★★★★ |
| **SVG sparkline/趋势图/动态效果** | ★★★★★ CSS/Canvas | ★★★★★ CSS/Canvas | ★★★★★ GPU 动画强 | ★★★★ 每帧重绘 | ★★★★★ | ★★★★★ |
| **贴近 React/JSX 心智(迁移成本)** | ★★★★(留 web 前端) | ★★★★★ RSX≈JSX | ★★ 全新 DSL | ★★ 范式不同 | ★★★☆ 反应式≠VDOM | ★★★★ 类 React |
| **单一代码库出桌面+Web** | ★★★(壳复用,前端本就是 web) | ★★★★★ 一套 RSX | ★★★★ 同 .slint | ★★★★ 同源码 | ★★★ 需自接 Tauri | ★★★ 需自接 Tauri |
| **桌面优先(Mac+Win)成熟度** | ★★★★★ 业界基准 | ★★★★☆ | ★★★★☆ | ★★★★ | ★★★★☆(借 Tauri) | ★★★★(借 Tauri) |
| **打包/签名/公证** | ★★★★★ 最成熟 | ★★★★ `dx bundle`(已修 mac 签名/公证) | ★★★★ | ★★★ 需自接 | ★★★★★(Tauri) | ★★★★★(Tauri) |
| **包体积** | ★★★★★ 复用系统 WebView(~几 MB) | ★★★★★ 同左 | ★★★★ 自带渲染(中等) | ★★★★ 较小 | ★★★★★ | ★★★★★ |
| **热重载 / 开发体验** | ★★★★ 前端 HMR + Rust 重编 | ★★★★★ Subsecond 热补丁 Rust | ★★★★ Live-Preview | ★★★★ 改即重跑 | ★★★★ trunk HMR | ★★★☆ |
| **生态 / 社区活跃度** | ★★★★★ 最大 | ★★★★☆ 增长快 | ★★★★ 稳健(含商业) | ★★★★★ 海量(工具/游戏) | ★★★★ | ★★★ 放缓 |
| **作为"设计感强+文档式富UI"的总适配** | ★★★★(前端非 Rust 扣分) | ★★★★★ | ★★★ | ★★ | ★★★★☆ | ★★★★ |

---

## 4. 决策理由(为什么是 Dioxus 0.7)

**理由一:本原型的"重量"只有真 CSS 引擎扛得住,而 Dioxus 桌面端就是真 CSS 引擎。**
1619 条内联样式 + 暖纸色板 + 中文衬线混排 + SVG sparkline,本质是一份**手调 CSS 设计稿**。Dioxus Desktop 明确"**Full HTML/CSS support via wry**"——与 Tauri 同款 WKWebView/WebView2。这意味着你现有的 `style="..."` 几乎可逐条搬进 RSX 的 `style` 属性(或转 Tailwind 类),`{{ }}` 插值与 `<sc-for>` 直接对应 RSX 的 `{}` 与 `for` 循环。**设计还原度 = 浏览器级,迁移路径最短**。Slint/egui 则要把这套设计在另一套渲染模型里**逐像素重建**——对"设计感极强"的产品是最贵的路。

**理由二:它同时解决"前端必须是 Rust"与"心智别离 React 太远"。**
Tauri 满足设计还原度,但**前端是 JS**——直接撞硬约束。Dioxus 的 RSX 是 **Rust 宏内的 JSX**:组件、props、条件、列表、事件全部是 Rust,却保留了你团队熟悉的声明式 UI 写法。`dc-runtime` 当前就是"模板 DSL → React.createElement",迁到 RSX 是**同范式迁移**,而非换脑(egui 的 immediate-mode / Slint 的 `.slint` 都是换脑)。

**理由三:一套 RSX 真正同时出桌面与 Web,贴合"桌面优先、Web 次之、单库复用"。**
官方原文:*"ship full-stack web, desktop, and mobile apps with a **single codebase**"*。同一份 RSX:桌面跑 `wry` WebView(满足第一优先级),Web 编译成 WASM 操作 DOM(满足第二优先级),**且两端 CSS 完全一致**(都是浏览器内核)。配套 `dx` CLI 一条命令打 Web/Desktop/Mobile 包、`asset!()` 资源优化、Subsecond **Rust 运行时热补丁**(改 Rust 不重启即生效)、内建 Tailwind、Radix-UI 组件、`LLMs.txt`(给 AI 编码器的一手上下文,正好契合本产品"AI 原生"的开发方式)。

**理由四:风险可控,且有清晰的"退一步"。**
Dioxus 比 Tauri 年轻、API 仍在 0.x 演进(0.8-alpha 在路上)。但兜底很硬:**次选 = Tauri v2(最成熟的桌面壳/签名/分发)+ Leptos 或 Yew 写前端(Rust→WASM)**。这条路前端仍是 Rust、仍在 WebView 里渲染、设计还原度同样满分,只是桌面/Web 不像 Dioxus 那样天然同构、且多一层 Tauri IPC。换句话说:**Dioxus 赌的是"同构 + DX",输了也只是退回到"业界标准 Tauri 壳 + Rust 前端",不会退回到 JS。**

**理由五:为何明确排除 Slint 与 egui(对本原型)。**
- **egui**:immediate-mode,样式系统官方自承*"not yet as powerful as CSS"*,布局每帧重算——对**文档式、长滚动、精排版**的 dashboard 是其最弱场景;暖纸渐变叠层、精细圆角阴影、中文混排都要手写绘制代码。**范式与目标背道而驰**。
- **Slint**:技术过硬(GPU、嵌入式、Figma 插件、免费商用许可),但它是**另一套设计语言(`.slint`),非 HTML/CSS**。把这份内联 CSS 稿搬过去 = **整套设计系统重写**;暖纸色板"够不够还原"答案是"能,但要逐个组件重画、自己调字体与阴影",**投入产出比对一个已成型的 CSS 原型最差**。若未来要做嵌入式/极致启动速度/无 WebView 依赖,再考虑 Slint。

---

## 5. "Tauri 前端非 Rust"张力 — 正式裁决

**事实:** Tauri v2 的设计哲学是**"前端任意 web 框架(JS/TS),业务逻辑 Rust"**(官网原文)。所以**裸 Tauri = 前端不是 Rust**,直接违背硬约束"语言必须是 Rust"。

**但这不等于必须放弃 WebView 内核。** 关键洞察:**"前端是 Rust" 与 "用 WebView 渲染 HTML/CSS" 是两件正交的事**。把两者拆开,有三档可选:

| 方案 | 前端语言 | 渲染内核 | 满足"语言用 Rust" | 设计还原度 |
|---|---|---|---|---|
| 裸 Tauri + JS/React | **JS/TS** ✗ | WebView | ✗ 不满足 | ★★★★★ |
| **Dioxus(首选)** | **Rust RSX** ✓ | WebView(`wry`) | ✓ 满足 | ★★★★★ |
| **Tauri + Leptos/Yew(次选)** | **Rust→WASM** ✓ | Tauri WebView | ✓ 满足 | ★★★★★ |

**裁决:**
1. **拒绝**"裸 Tauri + JS 前端"——它确实违背"语言用 Rust"。
2. **首选 Dioxus**:RSX 即 Rust,桌面端内核同样是 WebView(`wry`),既守住 Rust 约束又保住设计还原度,且额外拿到"一套代码出桌面+Web"。
3. **次选 Tauri + Leptos/Yew**:若 Dioxus 0.x 的某个坑挡路,就用最成熟的 Tauri 壳负责签名/分发,前端换成 Rust 的 Leptos/Yew(编译成 WASM 跑在 WebView 里)。**前端依旧是 Rust。**
4. 唯一会让你**重新接受 JS 前端**的场景:团队已有大量可复用的 React/TS 组件且工期极紧、又确实想要 Tauri 最成熟的分发——此时"裸 Tauri + JS"是务实捷径,但它是对硬约束的**妥协**,需显式承认。

> 一句话:**Tauri 的"前端非 Rust"是它的默认姿势,不是它的唯一姿势,更不是 WebView 路线的必然。** 选 Dioxus(或 Tauri+Leptos)即可"既要 Rust 前端、又要 WebView 的设计还原度"。

---

## 6. 何时改选(触发条件)

| 触发信号 | 从首选改到 |
|---|---|
| Dioxus 0.7→0.8 的破坏性变更/回归阻塞排期;或桌面端 `wry` 在某 Win/Mac 版本上有阻断性 bug | **Tauri v2 + Leptos**(壳最稳,前端仍 Rust) |
| 需要把现有 **React/TS 组件**大规模复用、且工期压死 | **Tauri v2 + 现成 web 前端**(显式接受"前端非 Rust"的妥协) |
| 团队更熟"细粒度反应式"且 Web(SSR/SEO)是重点 | **Leptos**(可 + Tauri 出桌面) |
| 产品转向**嵌入式/车机/极致启动/无 WebView 依赖**,或想要 GPU 原生动画上限 | **Slint** |
| 只做内部工具/调试面板、不在意像素级设计 | **egui/eframe**(开发最快) |
| 想试"无 webview 的纯 Rust 自绘但仍写 HTML/CSS" | 观望 **Dioxus Native / Blitz**(2026 仍实验,**暂不投产**) |

---

## 7. 推荐栈下的打包 / 签名 / 分发 与 Web 部署

### 7.1 桌面打包(Dioxus,`dx bundle`)
- **命令面**:`dx bundle --platform desktop --release`,产物 macOS=`.app`/`.dmg`,Windows=`.msi`/`.exe`(NSIS)。Dioxus CLI 底层复用 Tauri 系打包生态。
- **依赖**:macOS=系统 WKWebView(零运行时);Windows=**WebView2 Runtime**(Win11 自带;Win10 需 Evergreen Bootstrapper,打包时配置自动拉取或随包分发)。

### 7.2 macOS 签名 + 公证(notarization)
- 需 **Apple Developer 账号** + **Developer ID Application** 证书(分发到 App Store 之外用 Developer ID,不是 Mac App Store 证书)。
- 流程:`codesign --deep --options=runtime`(开启 **Hardened Runtime**)→ 用 **`notarytool`**(`xcrun notarytool submit`,旧 `altool` 已弃用)提交公证 → **`xcrun stapler staple`** 把票据钉进 `.app`/`.dmg`。
- **实测利好**:Dioxus **0.7.8 已修复 "macos signing/notarization"**(release notes 原文 *"Fix macos signing/notarization (0.7 backport)"`),说明 `dx` 的公证链路是被维护的已知路径。
- **坑**:① 公证只接受 **Developer ID + Hardened Runtime + 安全 timestamp**,缺一即拒;② 用到麦克风/相机/网络等需在 entitlements/`Info.plist` 声明;③ 自建更新需 `--options=runtime` 一致,否则 Gatekeeper 拦截;④ Apple Silicon + Intel 建议出 **universal2** 或分架构双包。

### 7.3 Windows 签名
- 用 **Authenticode** 代码签名证书(`signtool sign /fd sha256 /tr <时间戳服务器> /td sha256`)。**2023 起 OV 证书要求存于硬件令牌/HSM**;EV 证书可更快积累 SmartScreen 信誉。
- **坑**:① 新证书初期仍可能触发 **SmartScreen** 警告,需时间或 EV 累积信誉;② **WebView2** 缺失时的引导;③ MSI 与 NSIS 二选一,自动更新(见下)要与之匹配。

### 7.4 自动更新
- Dioxus/Tauri 生态可用 **updater 插件**(签名校验 + 版本清单 `latest.json`);需用私钥签更新包、客户端内置公钥校验,防止投毒。CI 出包后把签名产物推到对象存储 + 更新清单端点。

### 7.5 Web 部署(**已降级:以后也许,非 MVP**)

> 桌面唯一后,Web 整块移出 MVP,省下 ~12–26 人天(WASM 包体工程 / SSR+瘦后端 / CJK subset+FOUT / provider 代理服务 / IndexedDB adapter)。**留口零成本**:内核保持零-UI + WASM 可编译(CI `wasm32` check),Store trait 可换实现,每表留 `updated_at + rev/SyncCursor` → 未来回归无需迁移 schema。下列细节留待「重启 Web 时」参考。
- **同一份 RSX** `dx bundle --platform web`(或 `dx serve --platform web` 本地),产物是 `wasm` + JS 胶水 + `assets/`,作为**静态站**部署到任意 CDN/静态托管。
- **坑**:① **WASM 包体**——开 `--release`、`wasm-opt`、Dioxus 的 **WASM-Split/懒加载** 压首包;② 服务端必须发 `application/wasm` MIME + 合理缓存;③ **CJK 字体**(Noto Serif/Sans SC)体积大,**务必子集化(subset)+ `font-display: swap`**,否则首屏白字;④ 若用 SSR/hydration,需要一个 Rust 服务端(Axum),纯静态则用 CSR 即可;⑤ SVG sparkline 在 Web/桌面同构,无需两套绘制代码。

### 7.6 CI 建议
- macOS 签名/公证必须在 **macOS runner**;Windows 签名在 **Windows runner**(令牌/HSM 通过云签名服务或自托管 runner)。证书与私钥走 CI Secrets,**绝不入库**。三平台矩阵:`macos-arm64 / macos-x64(或 universal) / windows-x64 / web`。

---

## 附录 · 关键版本与出处(2026-06-26 实测)

- Tauri **2.11.3**(2026-06-17),`wry` 0.55.1,`tao` 0.35.3 — crates.io API
- Dioxus **0.7.9** 稳定 / **0.8.0-alpha.0**(2026-05-19)— crates.io API + GitHub Releases;Dioxus 0.7 发布博客(Subsecond/Dioxus Native/Blitz/Tailwind/single codebase 原文);`dioxus-desktop` README("Full HTML/CSS support via wry and tao");Blitz README("would not yet recommend building apps with it";Stylo+Taffy+Parley);0.7.8 release notes("Fix macos signing/notarization")
- Slint **1.17.0**(2026-06-24)— crates.io + GitHub Releases;slint.dev(GPU 渲染/Figma 插件);GitHub 许可说明(Royalty-free / GPLv3 / Commercial)
- egui / eframe **0.35.0**(2026-06-25)— crates.io;egui README("not yet as powerful as say CSS"、immediate-mode、CJK 字体)
- Leptos **0.8.20**(2026-06-25)— crates.io;Leptos README(全栈/CSR/SSR/细粒度反应式)
- Yew **0.23.0**(2026-03-10)— crates.io
- wgpu **29.0.3** / Tauri v2 官网("Write your frontend in JavaScript, application logic in Rust";Sidecar / Embedding External Binaries)

> 方法说明:本轮 WebSearch/WebFetch 工具后端(模型)临时不可用,改用 `curl` 直接拉取 crates.io API、GitHub Releases API 及各官方页面/README 原始内容,逐项解析。版本号为 API 实时返回,能力描述均引自官方原文,未凭记忆。
