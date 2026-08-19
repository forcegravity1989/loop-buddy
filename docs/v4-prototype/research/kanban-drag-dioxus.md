# 穿刺笔记:计划屏 Issue 卡片拖拽,在 Dioxus 0.7 + wry 里能不能做(2026-08-19)

> **30 秒导读**:这是一篇**预研**,回答 2026-08-19 内部专家评审提的一条要求——计划屏(六列看板)的 Issue 卡片要能拖动,提高易用性。设计事实源 `docs/v4-prototype/mvp-blueprint-draft.md` 已经拍板了**范围**(待拍-25:拖拽只用于排期——待办池⇄待办、列内排序;状态流转仍走按钮),但没定**怎么实现**。本文只答技术可行性:桌面壳是 Dioxus 0.7(硬钉 `=0.7.9`)+ wry WebView(macOS 用 WKWebView、Windows 用 WebView2)套壳的浏览器 HTML5 拖放 API,这层技术栈在 BW 里从没人用过拖放,坑在哪不知道。**结论先说:能做,活不大,但 Windows 上有一个真实的、必须处理的框架级冲突(下面事实 3);macOS 上没找到已知冲突。** 本文不改代码、不是设计文档,状态=预研,拍板权在 `mvp-blueprint-draft.md`(该文档待拍-25 已定的范围本文不重复讨论对错,只讨论怎么落地)。全文的判断都标了出处路径或命令,没有凭印象下结论的地方一律写"未验证"。

---

## 一句话结论

**能做,是小活**:Dioxus 0.7.9 的拖放事件(`ondragstart`/`ondragover`/`ondrop` 等)齐全,`prevent_default()` 是普通方法调用不用查历史写法,而且框架自己已经在 JS 层全局吞掉了 `dragover`/`drop` 的默认动作(事实 2)——这意味着"HTML5 拖放必须在 dragover 里阻止默认"这条常见坑,BW 不用自己操心。**真正要处理的一件事在 Windows**:wry 默认注册的原生文件拖放处理器会让 WebView2 屏蔽页面内的 HTML5 拖放事件,必须在桌面壳启动配置里加一行 `.with_disable_drag_drop_handler(true)`(事实 3)——BW 现在没用文件拖入窗口这个功能,加这行没有代价。V3 现有的六列看板(`crates/app-desktop/src/screens/op/issues.rs`)没有"周"概念、没有可拖动排序的列,要新增两条命令、一处 schema 迁移(§6 给出估算)。已在仓外用真实 `cargo build` 编译验证了拖放事件 API 可用(§5)。

---

## 1. 要回答的问题,和它在 V4 设计里的位置

`docs/v4-prototype/mvp-blueprint-draft.md:147`(第四轮专家反馈段)原文:

> 两列的定义——**待办池 = 还没排进任何一周的活**(想法、远端同步进来的 issue、agent 拆出来但没排期的草稿),**待办 = 已排进本周、等开工的活**……**拖拽做,但只做「排期」这类无副作用的动作**:待办池 ⇄ 待办(拖过去 = 排进本周 / 拖回来 = 移出本周,只改 `week_of`)、列内拖动排先后;**状态流转仍走卡片上的按钮**……把这些做成拖拽,误拖一下就起了会话或结了账,反而不好用

拍板记录在 `mvp-blueprint-draft.md:355`(待拍表第 25 行):"✅ 待拍-25 已定(第四轮专家反馈):看板拖拽只用于排期(待办池 ⇄ 待办、列内排序);状态流转仍走按钮;两列定义写在列头。"——**这条已经拍板,本文不重新论证要不要做,只回答技术上怎么做、代价多大。**

一句话解释给非专业读者:**HTML5 拖放**是浏览器/WebView 原生支持的一套"鼠标按下拖动、松开触发"的网页事件(`dragstart`/`dragover`/`drop` 等),BW 桌面壳的界面是用网页技术(HTML/CSS,靠 Dioxus 生成)画出来再塞进一个系统自带的迷你浏览器(WebView)里显示的,所以拖放要不要做、能不能做,取决于这套网页 API 在这个"迷你浏览器"套壳里好不好用。

---

## 2. 事实 1:Dioxus 0.7.9 的拖放事件面——齐全,`prevent_default()` 是普通方法

出处:`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-html-0.7.9/`(`find ~/.cargo/registry -maxdepth 3 -iname 'dioxus-html-0.7*'` 定位到的本机已下载源码,和 BW workspace 锁定的版本一致,见 `Cargo.lock`)。

- **事件齐全**:`src/events/generated.rs:41-48` 逐条注册了 `ondrag`/`ondragend`/`ondragenter`/`ondragexit`/`ondragleave`/`ondragover`/`ondragstart`/`ondrop`,类型是 `DragEvent = Event<DragData>`(`src/events/drag.rs:13`)。BW 要用的 `ondragstart`/`ondragover`/`ondrop` 都在。
- **`prevent_default()` 是普通方法,不是历史上的属性写法**:`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-core-0.7.9/src/events.rs:172-178` 就是 `pub fn prevent_default(&self) { self.metadata.borrow_mut().prevent_default = true; }`,直接 `evt.prevent_default()` 调用即可。`dioxus-html-0.7.9/src/attribute_groups.rs:219-221` 里能查到旧的 `prevent_default: "dioxus-prevent-default"` 属性字符串写法,但标了 `#[deprecated]`,注释明确写"大多数渲染器请改用 `dioxus_core::Event::prevent_default`"——0.7 不需要那套历史写法。
- **`DataTransfer::set_data`/`get_data` 在桌面壳上是"读能读、写是空操作"**:`dioxus-html-0.7.9/src/data_transfer.rs:108-152` 是 `SerializedDataTransfer`(桌面/移动端用的反序列化实现)——`set_data`/`clear_data`/`set_effect_allowed`/`set_drop_effect` 全部标注 `// No-op` 或注释掉的 `todo!()`,直接 `Ok(())` 什么也不做;`get_data`/`files()` 能读到 JS 侧已经序列化好的内容。也就是说:**在 `ondragstart` 里调用 `drag.data_transfer().set_data(...)` 不会真的把数据写进浏览器那个 `DataTransfer` 对象**,后面 `ondrop` 里读不到你想传的"这是哪张卡"。
- **BW 该怎么传"拖的是哪张卡"**:不用 `DataTransfer`,用一个 `Signal<Option<IssueId>>`——`ondragstart` 直接读闭包捕获的卡片 id(渲染时的 `for card in cards` 循环变量,本来就在闭包作用域里)写进这个 Signal,`ondrop` 读出来用。这是桌面单进程场景下最简单可靠的传法,§5 的编译验证用的就是这个模式,跑通了。

**判断**:API 面没问题,`prevent_default()` 用法已经是现代写法;唯一要绕开的坑是不要指望 `DataTransfer.set_data/get_data` 当跨列传参手段,改用 `Signal` 存"当前正在拖的卡片 id"。

---

## 3. 事实 2:框架自己已经在 `dragover`/`drop` 上全局 `preventDefault`——时序担忧不成立

这是最值得记的一条,因为它直接排除了任务背景里最担心的一个坑("HTML5 拖放必须在 dragover 里同步阻止默认,否则 drop 不触发;Dioxus 桌面壳的事件处理要经过一次 IPC/webview 往返,会不会来不及?")。

出处:`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-interpreter-js-0.7.9/src/js/native.js`(压缩后的构建产物;可读源码在同目录 `src/ts/native.ts:53,69,91`,逻辑与压缩版一致)。`NativeInterpreter.initialize()` 里原样有这几行(从压缩产物摘出、加了换行,逻辑未改一字):

```js
window.addEventListener("dragover", function (e) {
  if (e.target instanceof Element && e.target.tagName != "INPUT") e.preventDefault();
}, false);
window.addEventListener("drop", function (e) {
  if (!(e.target instanceof Element)) return;
  e.preventDefault();
}, false);
```

也就是说:**每个 Dioxus 桌面窗口一初始化,就在 `window` 级别注册了两个监听器,对几乎所有元素(`INPUT` 之外)的 `dragover`/`drop` 无条件同步调用 `preventDefault()`**——不等 Rust 侧任何响应,浏览器一收到事件立刻在 JS 里就地拦掉默认动作。这一步跟组件上写不写 `ondragover: |e| e.prevent_default()` 完全无关,框架自己已经做了。

再往下一层看事件怎么真正传到 Rust:`dioxus-desktop-0.7.9/src/webview.rs:56-95`(`handle_event`)配合 `dioxus-interpreter-js-0.7.9/src/js/native.js` 里的 `sendSerializedEvent`/`handleVirtualdomEventSync` ——组件上挂的 `ondragover`/`ondrop` 回调,走的是**同步 XHR**:`handleVirtualdomEventSync` 函数原文是 `xhr.open("POST", endpoint, false)`(第三个参数 `false` = 同步,阻塞 JS 主线程直到 Rust 处理完并返回);Rust 侧算完 `event.prevent_default()` 有没有被调用后,把结果包成 `SynchronousEventResponse`(`dioxus-desktop-0.7.9/src/webview.rs:628-636`,类型文档注释就写"a synchronous response to a browser event which may prevent the default browser's action")传回来,JS 再按 `response.preventDefault` 决定要不要再调一次 `event.preventDefault()`。**这条链路本身就是同步阻塞的,不存在"Rust 处理慢了、preventDefault 生效前浏览器已经执行完默认动作"这种时序竞争。**

**判断**:两层保险都在——① 框架级全局监听器已经无条件 preventDefault 了 dragover/drop,② 组件级的事件处理走同步 XHR、没有异步时序问题。BW 该在 `ondragover`/`ondrop` 里正常写 `evt.prevent_default()`(和标准网页写法一样,§5 的探针代码就是这么写的),不需要额外的时序技巧。

---

## 4. 事实 3:wry/WebView 层——macOS 目前没查到冲突,Windows 有一个真实的、必须处理的框架级坑

出处:`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wry-0.53.5/` 与 `dioxus-desktop-0.7.9/`(和 BW `Cargo.lock` 钉住的同一版本)。

**Windows(WebView2)——真坑,必须处理**:`dioxus-desktop-0.7.9/src/config.rs:164-166`:

```rust
/// Set whether or not the file drop handler should be disabled.
/// On Windows the drop handler must be disabled for HTML drag and drop APIs to work.
pub fn with_disable_drag_drop_handler(mut self, disable: bool) -> Self {
    self.disable_file_drop_handler = disable;
    self
}
```

这行文档注释是官方原话:**Windows 上,要让 HTML 原生拖放 API(也就是 BW 要用的 `ondragstart`/`ondragover`/`ondrop`)能工作,必须禁用这个"文件拖放处理器"**。往下看它默认是不是开着的——`dioxus-desktop-0.7.9/src/config.rs:73/127` 里 `disable_file_drop_handler: bool` 默认值是 `false`(即处理器**默认开着**),`webview.rs:411-412`:`if !cfg.disable_file_drop_handler { webview = webview.with_drag_drop_handler(file_drop_handler); }` ——默认情况下这个处理器会被注册。

这个处理器具体做什么、为什么会挡住 HTML 拖放,代码自己写了原因(`webview.rs:320-352`,`file_drop_handler` 闭包内的注释,原文摘录):

```rust
if cfg!(not(windows)) {
    // Update the most recent file drop event ...
    file_hover.set(evt);
} else {
    // Windows webview blocks HTML-native events when the drop handler is provided.
    // The problem is that the HTML-native events don't provide the file, so we need this.
    // Solution: this glue code to mimic drag drop events.
    ...
}
```

翻译:这是 wry/WebView2 的一个已知限制——**只要注册了 wry 的原生拖放处理器(`with_drag_drop_handler`,用来接收操作系统级别"文件从 Finder/资源管理器拖进窗口"这类事件、拿到完整文件路径),WebView2 在 Windows 上就会连带屏蔽掉页面自己的 HTML `dragover`/`drop` 事件**——这不是 BW 的 bug,是 WebView2 平台行为,dioxus 团队为了保留"文件拖进窗口"这个能力,写了一段"glue code"去模拟(通过 `dioxus-interpreter-js` 的 `handleWindowsDragOver`/`handleWindowsDragDrop`,在鼠标下的元素上人工 `dispatchEvent(new DragEvent(...))` 伪造一个网页事件),但这段模拟代码构造的 `DataTransfer` 里塞的是一个写死的假文件(`new File(["content"],"file.txt",...)`,见 `dioxus-interpreter-js-0.7.9/src/js/native.js` 里 `handleWindowsDragDrop` 函数),明显是给"文件拖入窗口"这个场景准备的,不是给"页面内两个 `<div draggable>` 互拖"这种场景准备的。

**结论**:Windows 上默认配置(`disable_file_drop_handler=false`,BW app-desktop 目前就是这个默认——`grep -rn "Config::new\|with_disable_drag_drop\|file_drop" crates/app-desktop/src/` 只命中了 `main.rs:57` 的 `dioxus::desktop::Config::new().with_window(...)`,没有任何拖放相关配置)下,**页面内的卡片拖放大概率不会正常触发**,必须在 `Config::new()` 链上加 `.with_disable_drag_drop_handler(true)`。BW 现在完全没有用"文件拖进窗口"这个功能(同一次 grep 在 `crates/app-desktop/src/` 下对 `file_drop`/`FileDrop` 零命中),所以关掉它没有已知代价。

**macOS(WKWebView)——没查到框架级冲突,但有一处未决(见开放问题①)**:`file_drop_handler` 闭包里 `cfg!(not(windows))` 分支只是把最新的 `DragDropEvent` 存进一个 `file_hover` 信号,不做任何"吞掉事件"的操作(`webview.rs:322-330`);wry 的 `DragDropEvent`(`wry-0.53.5/src/lib.rs:2167-2183`,`Enter{paths,position}`/`Over{position}`/`Drop{paths,...}`/`Leave` 四个变体)本身只对应"操作系统级别文件被拖进窗口"这条通道,和"页面内 `<div draggable>` 元素互相拖动"是两条不相关的通道——WKWebView 处理后者完全在网页引擎内部,不经过这个 wry 回调。**源码层面没有找到 macOS 上会拦截页面内拖放的机制**,但"WKWebView 桌面(非 iOS)环境下 `draggable="true"` 的普通 `<div>` 是否开箱即用"这一具体平台行为,本次没能找到官方一手文档确认(联网检索到的资料集中在 iOS WKWebView——那是触屏环境,HTML5 拖放本来就要靠 shim 补,和 macOS 桌面 WKWebView 用鼠标拖动是两回事,不能直接套用;详见开放问题①)。

---

## 5. 验证:仓外最小可跑编译(真实 `cargo build`,不是纸面判断)

按任务要求,试验代码放在仓外:`/private/tmp/claude-501/.../scratchpad/dnd-probe/`(不在本仓 `crates/` 下,未 commit)。`Cargo.toml` 依赖 `dioxus = { version = "=0.7.9", features = ["desktop"] }`,并额外执行 `cargo update -p dioxus-desktop --precise 0.7.9` 把 `dioxus-desktop` 精确钉到和 BW 主仓 `Cargo.lock` 完全相同的版本(默认解析会拿到 `0.7.10`;`dioxus-html` 因为 `dioxus-document` 的版本约束锁不到 0.7.9、停在 0.7.10,但 `diff` 了 `dioxus-html-0.7.9` 和 `-0.7.10` 的 `src/events/drag.rs` 与 `src/data_transfer.rs` 两个文件,**逐字节相同**,不影响本文任何结论)。

代码内容:两列(待办池 / 待办)、每列若干卡片,`draggable: "true"` + `ondragstart`(把卡片 id 写进 `Signal<Option<u32>>`)+ 目标列 `ondragover`(`evt.prevent_default()`)+ `ondrop`(读 Signal、决定 id 归哪一列、`evt.prevent_default()`)——就是 §2 建议的"用 Signal 代替 DataTransfer"那套写法。

```bash
cd /private/tmp/.../scratchpad/dnd-probe && cargo build
```

结果:**`Finished \`dev\` profile [unoptimized + debuginfo] target(s)`,退出码 0。** 首次全量编译(含 wry/tao/dioxus-desktop 等全部依赖树)1 分 06 秒;把 `dioxus-desktop` 精确钉到 0.7.9 后增量重编只需 6.33 秒。唯一的警告是 `block v0.1.6` 的 future-incompatibility 提示(wry 的间接依赖,和拖放功能无关)。**这证明 §2 描述的事件 API 组合(`draggable` 属性、`ondragstart`/`ondragover`/`ondrop`、`Signal` 传 id、`prevent_default()`)在 Dioxus 0.7.9 desktop 下确实能通过类型检查和编译**——本次验证到此为止,没有做窗口内实际拖动的交互测试(任务范围是"cargo build 能过就算成功",没有要求跑起来点)。

---

## 6. V3 现状与落地建议

### 6.1 现状(只读,未改代码)

- **六列看板**已经存在:`crates/app-desktop/src/screens/op/issues.rs:143-150` 定义 `cols: [(IssueStatus, &str); 6]` = 待办池/待办/进行中/评审中/已完成/阻塞,`:152-164` 按状态分组渲染,每列内部顺序是 `op.issues` 原始顺序(见下"排序"一条)。
- **状态怎么改**:卡片上的按钮直接发 `Command::TransitionIssue { id, status }`(`issues.rs:434/439/593/916/925`)、`Command::BlockIssue { id, reason }`(`issues.rs:457`)、`Command::AssignIssue { id, assignee }`(`issues.rs:414`)、`Command::MergeIssuePr { id }`(`issues.rs:583/905`)。合法转移表是唯一事实源:`crates/bw-core/src/model.rs:1636-1661` 的 `IssueStatus::can_transition_to`,例如 `(Backlog, Todo)`、`(Todo, InProgress)` 等都在里面——待拍-25 的"拖到 Todo/InProgress 等列只提示"这条,技术上就是"拖放不触发 `TransitionIssue`,只在允许的两列间(Backlog⇄Todo)触发新命令"。
- **没有"周"概念**:`grep -rn "week_of" crates/` 全仓零命中;`issue` 表结构(`crates/bw-store/src/schema.sql`,`CREATE TABLE IF NOT EXISTS issue (...)`)里没有周相关列,也没有排序列——`ORDER BY` 只在 `list_issues`(`crates/bw-store/src/sqlite.rs:2737`)里写死 `ORDER BY number ASC`(按创建顺序的自增编号),不支持人工调序。这印证了 `mvp-blueprint-draft.md` §6 提到 V4 要新加 `week_of` 是真的新概念,不是遗漏。

### 6.2 建议新增的命令

沿用 `crates/bw-app/src/command.rs` 里 `Command` 枚举现有的写法(如 `AssignIssue`/`BlockIssue`,`command.rs:602-638` 附近),建议:

```rust
/// 待拍-25:排期专用命令,和状态流转(TransitionIssue)彻底分开——
/// 拖放只应该发这个,绝不应该让拖放路径间接触发 TransitionIssue。
ScheduleIssue {
    id: IssueId,
    week_of: Option<i64>,   // None = 移回待办池;Some(周起始时间戳) = 排进该周
},
/// 列内拖动排序;rank 是浮点数或整数间隙排序(常见做法:相邻两卡 rank
/// 取中间值,避免每次拖动重排全列)。
ReorderIssue {
    id: IssueId,
    after: Option<IssueId>, // 拖到这张卡之后;None = 排到列首
},
```

**为什么不复用 `TransitionIssue`**:待拍-25 明确"拖拽只用于排期,状态流转仍走按钮"——如果排期动作也走 `TransitionIssue`,就得在 `can_transition_to` 表里塞进"Backlog⇄Todo 通过拖拽也算合法"这类特例,容易和"六列看板的 Backlog→Todo 本来就是合法状态转移"这条已有规则混在一起,以后很难区分一次 `TransitionIssue { Todo }` 到底是人点按钮推进的还是拖放排期带来的副作用。分开成独立命令,`week_of`/`rank` 只改排期字段、不碰 `status`,职责边界干净,也符合 CLAUDE.md「不为向后兼容留旧路径」的一贯做法——不是给现有命令加隐藏分支,是新开一条干净的路径。

### 6.3 Schema 迁移(双守卫,CLAUDE.md 核心纪律第 5 条已经点名的坑)

对照 `crates/bw-store/src/sqlite.rs` 里已有的 `add_column_if_missing` 用法(例如 `:143` 的 `add_column_if_missing(&pool, "issue", "settled_at", "INTEGER")`),新增列要同时改两处:

1. `crates/bw-store/src/schema.sql` 的 `issue` 表定义里加 `week_of INTEGER` 和 `rank REAL`(或等价的排序列);
2. `sqlite.rs` 里加两行 `add_column_if_missing(&pool, "issue", "week_of", "INTEGER").await?;` / `add_column_if_missing(&pool, "issue", "rank", "REAL").await?;`——只改 `schema.sql` 存量库不会自动加列,这条 CLAUDE.md 已经写死。

同时 `list_issues`(`sqlite.rs:2716-2747`)的 `SELECT` 字段列表和 `ORDER BY number ASC` 要跟着改:`SELECT` 加 `week_of, rank` 两列,`ORDER BY` 改成先按 `rank`(列内顺序)再按 `number`(兜底,`rank` 为空时不乱序)。`bw-core::model::Issue` 结构体(`model.rs:1694-` 起)也要加对应字段,VM 层(`ui` crate 的 `IssueDetailVm`/看板用的 VM)按需加。

memory 记录里提过一个真实踩坑(`project_id 进 schema 但读侧全链路从未接上`)——**这条链路(schema → 领域结构体 → SELECT → VM)必须三处一起改**,只改 schema 不改读侧,列就是"进了库、界面上看不到"的死列,是这个仓库已经真实发生过的问题,不是本文猜的风险。

### 6.4 UI 改动量估计

- **拖放事件挂载**:每张卡加 `draggable: "true"` + `ondragstart`/`ondragend`(§5 探针验证过的写法),两列(待办池、待办)容器加 `ondragover`(`prevent_default()`)+ `ondrop`(读 `Signal<Option<IssueId>>`、发 `ScheduleIssue`/`ReorderIssue`)。`issues.rs` 现有的看板渲染循环(`:263-614`)结构不用大改,是在现有 `div` 上加几个事件属性 + 一个新增的拖拽状态 `Signal`,量级是几十行。
- **拖到非法列的提示**:待拍-25 原文"拖到这些列上时给一句提示「用卡片上的按钮」"——`ondrop` 里判断目标列是不是 Backlog/Todo 之外的列,不是就弹一条 toast(BW 已有 toast/通知机制,`k.send`/`UiNote` 通道,不是新基建),量级是十几行条件分支。
- **合计**:在"六列看板已经存在、命令总线已经存在"的前提下,新增拖放是**小到中等**的一次改动——新命令(2 个,`bw-app` 侧含 dispatch 分支)+ schema 双守卫迁移 + `issues.rs` 里加事件属性和一个新 Signal + 一处提示逻辑,不是新起一个子系统。真正花时间的不是拖放本身,是"待办池/待办"两列语义在计划屏(周视角,`mvp-blueprint-draft.md:241` 描述的左栏周列表 + 中栏六列看板)里怎么和已有的"选中周"状态联动——这部分是 V4 计划屏本身要设计的范围,不是拖放这个技术点的范围。

---

## 7. 易用性细节(供落地时参考,非本文强制结论)

- **键盘/右键菜单替代路径,建议同时给**:HTML5 拖放没有原生键盘可达性(视障或键盘用户没法用 Tab+方向键完成拖放),待拍-25 的"排进本周/移出本周"本质是改一个字段,完全可以在卡片的右键菜单或一个"…"按钮里同时放"排进本周"/"移出本周"两个菜单项,直接发同一条 `ScheduleIssue` 命令——不依赖拖放手势,成本是复用已有的命令,只加菜单 UI。
- **拖放视觉反馈**:建议至少做到——拖动时源卡片降低透明度、目标列(待办池/待办)在 `ondragenter`/`ondragover` 时高亮边框、非法列(进行中/评审中/已完成/阻塞)在拖动过程中显示"不可放"样式(比如变灰 + `cursor: not-allowed`,配合前述的 toast 提示)。这些都是纯 CSS/条件渲染,不涉及新的数据或命令。
- **触控板误拖防护**:浏览器原生 HTML5 拖放本身有一个隐式阈值(移动几像素才真正触发 `dragstart`,防止单击被误判成拖动开始),Chromium/WebKit 都有;不需要 BW 自己实现拖动阈值判断。真正的误触风险更多在"手滑拖到相邻列"——这个靠上面说的"非法列拖放只提示不生效"兜底,不需要额外的手势库。

---

## 8. 不做什么

**不做状态流转类拖拽**(把卡片拖到"进行中"/"评审中"/"已完成"/"阻塞"列来触发 `TransitionIssue`/`BlockIssue`/`MergeIssuePr`)。原文引用 `mvp-blueprint-draft.md:147`:"状态流转仍走卡片上的按钮(▶开工要真起 agent、评审中由 MR 推导、✓完成是铁律里「人显式点」的那一下、⛔阻塞要填原因)——把这些做成拖拽,误拖一下就起了会话或结了账,反而不好用"。这条已经在待拍-25 拍板,本文的技术设计(§6.2 单独开 `ScheduleIssue`/`ReorderIssue`,不复用 `TransitionIssue`)就是为了从命令层面把"拖放"和"状态流转"这两件事physically 分开,不给"拖放不小心触发状态流转"留任何路径。

---

## 9. 开放问题(≤3)

1. **macOS 桌面(非 iOS)WKWebView 对通用 `<div draggable="true">` 的支持程度,没有找到权威一手文档确认**——联网检索(`WebSearch`)只找到 iOS WKWebView(触屏、需要 shim,如 `ios-html5-drag-drop-shim` 项目)和历史上 Safari `-webkit-user-drag` CSS 属性的零散资料,没找到专门针对"macOS 桌面 WKWebView 承载的 Web 内容,鼠标拖动一个普通 `<div>`"这个具体场景的官方说明。§4 从 wry/dioxus-desktop 源码层面确认了 macOS 分支不存在会拦截页面内拖放的框架级机制,但"WKWebView 引擎本身对 draggable 属性的实现是否完整"这一层,本文没能独立核实,建议落地前用真机点一次(computer-use 对 BW 自己打包的 app 目前 click 受阻,见 memory「BW computer-use BWDev.app」,可能需要人工在自己屏幕上点一次来确认,而不是指望自动化工具点出结果)。
2. **`ReorderIssue` 的 `rank` 用什么类型**(浮点数插入排序 vs 整数间隙排序 vs 每次拖动整列重算序号)本文只给了方向,没有选定方案——这是纯后端实现细节,不影响本文"能不能做"的结论,但落地前需要单独定。
3. **Windows 上关掉 `disable_file_drop_handler` 之后,BW 现在唯一潜在依赖它的路径是否真的为零**——本文用 `grep` 确认了 `crates/app-desktop/src/` 里没有 `file_drop`/`FileDrop` 相关代码,但没有检索 `crates/bw-app` 或其他 crate 是否有计划中(未落地)的文件拖入窗口功能;落地前建议在改动的 PR 里过一遍全仓 `grep`,不要只看 app-desktop。

---

**三行结论(交回)**:

1. 能做,活不大:Dioxus 0.7.9 拖放事件齐全,`prevent_default()` 写法现代,框架自己已经在 JS 层全局帮你阻止了 `dragover`/`drop` 的默认动作,不存在"来不及 preventDefault"的时序坑;仓外真实 `cargo build`(与 BW 完全同版本 `dioxus-desktop=0.7.9`)编译通过,验证了拖放事件 + `Signal` 传卡片 id 这套写法可行。
2. 真正要处理的是 Windows 一个框架级冲突:wry 默认注册的文件拖放处理器会让 WebView2 屏蔽页面内拖放事件,必须在启动配置加 `.with_disable_drag_drop_handler(true)`(dioxus-desktop 官方文档注释原话确认);BW 现在没用文件拖入窗口功能,加这一行没有代价,但不加,Windows 用户大概率拖不动卡片。
3. V3 看板要新增两条命令(`ScheduleIssue`/`ReorderIssue`,故意不复用 `TransitionIssue`)和一次 schema 双守卫迁移(`week_of`/`rank` 两列),六列看板本身、状态转移的按钮逻辑都不用动——量级是小到中等的一次增量,不是新起系统;真正的设计工作在"待办池/待办"两列语义怎么和计划屏的"选中周"状态联动,这部分是 V4 计划屏设计范围,不是拖放技术本身的范围。
