//! `bw-next` —— next 切片五E 桌面壳(design-s5-hexpanel.md §4)。两屏
//! (六段总控 `BW_PANEL=hex` / 待人处理 `BW_PANEL=attention`),壳只发命
//! 令收事件,不做任何推导(见 [`kernel`] 模块文档)。
//!
//! **深链渲染证明**(§4.4):`BW_DB`/`BW_OPEN`/`BW_PANEL` 三个环境变量,
//! stderr 打 `[BW_OPEN] "项目名" -> panel=Hex 指标=N 观测=N 运行=N
//! 待人处理=N`。`BW_OPEN` 对不上任何项目时,进程在**打开任何窗口之前**
//! 以非零码退出(§4.4「一处新增」)——见 [`kernel::DeepLinkOutcome`] 文
//! 档「如实的偏差」一节,讲清楚这个判断为什么发生在 `main()` 而不是
//! (像旧壳那样)完全交给编排层线程 fire-and-forget。

#![forbid(unsafe_code)]

// `escape_hatch` 住在这个 crate 的库面(`src/lib.rs`),不是这个二进制的
// 私有子模块——`examples/seed_shell_demo.rs`(深链验证造数脚本)要调同
// 一份代码打印真实的续接命令,库面是示例唯一够得着的入口。`kernel`/
// `screens`/`theme` 三个仍然是这个二进制自己的私有模块,没有第二个调用
// 方需要复用它们。
mod kernel;
mod screens;
mod theme;

use dioxus::prelude::*;
use kernel::{DeepLinkOutcome, Kernel, Panel, ShellCommand, UiNote, Vm};
use std::sync::OnceLock;

/// `main()` 在调用 `dioxus::launch` 之前把已经起好的 [`Kernel`] 把手存进
/// 这里,`Root()` 组件挂载时读出来——同旧壳 `Root()` 直接读
/// `std::env::var(...)` 拿深链参数是同一类"组件挂载时读外部一次性状
/// 态"的手法,这里只是把"读一个环境变量字符串"换成"读一个已经起好的、
/// 内部含有实时 channel 的把手"。只写一次(`main()` 里),`Root()` 只读。
static KERNEL: OnceLock<Kernel> = OnceLock::new();

/// **`fn main() -> Result<(), String>`,不是让 `main` 返回 `()` 再手动调
/// 用标准库那个"直接杀掉当前进程、带一个退出码"的函数**——两处如实原
/// 因:①`guard-no-direct-process.sh`(design §10.1 第 3 条,本片把新壳
/// 目录也纳进它的覆盖面)禁的是那一整族模块路径(不只起子进程那个类
/// 型,脚本注释写明连那个直接终止函数这类旁路也一并锁死),字面写出那
/// 一行调用会被它拦下,而这条禁令原本要拦的是"起外部进程绕开连接器接
/// 口",和"这个进程自己控制退出码"是两回事——不该为了满足一条误伤的字
/// 面匹配去改脚本的正则(那样会真的削弱它对"起外部进程"这条主要意图的
/// 拦截力);②Rust 标准库给 `Result<T, E: Debug>` 提供了一个内置的收尾
/// 实现——`main` 返回 `Err` 时,运行时打印错误并以非零码退出,整个过程
/// **不需要在源码里出现任何一处那一族模块路径**,不是绕开门禁的取巧写
/// 法,是这条需求在 Rust 里本来就该用的机制,反而比手动调那个直接终止
/// 函数更符合"结构化控制流,没有裸退出"这条门禁的深层意图。
fn main() -> Result<(), String> {
    // 深链前置探测(见 kernel::DeepLinkOutcome 文档)——在这里同步等,是
    // 因为这一刻还没有调用 `dioxus::launch`,没有任何窗口被创建;`BW_OPEN`
    // 对不上项目时,进程可以在这里干净地以非零码退出,不留下任何 GUI 痕
    // 迹。没有配 `BW_OPEN` 时这条等待也会发生(编排层线程无论如何都会发
    // 一次 `Ready`),但正常交互式启动的这次等待通常只有几十毫秒(本地
    // SQLite 打开 + 一轮查询),不是一个用户能感知的延迟。
    let (outcome_tx, outcome_rx) = std::sync::mpsc::channel::<DeepLinkOutcome>();
    let handle = kernel::spawn(outcome_tx);
    match outcome_rx.recv_timeout(std::time::Duration::from_secs(30)) {
        Ok(DeepLinkOutcome::NotFound) => {
            // `[BW_OPEN] "xxx" NOT FOUND` 已经在编排层线程里打过了(见
            // kernel::resolve_deep_link);这里只需要把退出码带上去。
            return Err("BW_OPEN 指定的项目不存在,已在上面 [BW_OPEN] … NOT FOUND 一行说明,进程未打开任何窗口".to_string());
        }
        Ok(DeepLinkOutcome::Ready) => {}
        Err(_) => {
            eprintln!(
                "[BW_OPEN] 内核线程 30 秒内未完成首次启动,如实降级继续(不影响退出码判定,只是没等到)"
            );
        }
    }
    let _ = KERNEL.set(handle);

    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("Builders' Workbench · Next")
                    .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 860.0)),
            ),
        )
        .launch(Root);
    Ok(())
}

/// 静态头部元素——无 props 的子组件不会重渲染(`document::Style` 的 diff
/// 不受支持),同旧壳 `GlobalChrome` 同款先例。
#[component]
fn GlobalChrome() -> Element {
    rsx! {
        document::Title { "Builders' Workbench · Next" }
        document::Style { {theme::GLOBAL_CSS} }
    }
}

#[component]
fn Root() -> Element {
    let kernel = KERNEL
        .get()
        .expect("main() 必须在 dioxus::launch 之前把 Kernel 存进 KERNEL")
        .clone();
    // next 切片 5.5:`Kernel` 经 context 供全树读取——`screens::terminal::
    // TerminalWidget` 用它订阅字节流(`use_context::<Kernel>()`),不当
    // prop 传:`Kernel` 内含 tokio channel 把手,没有(也不该造一个假
    // 的)`PartialEq`,`#[component]` 的 props diff 要求每个 prop 都实现
    // 它——同 v1 `TerminalWidget` 自己 `use_context::<Kernel>()` 的既有
    // 先例(`kernel` 模块文档「切片 5.5」一节)。
    use_context_provider({
        let kernel = kernel.clone();
        move || kernel
    });
    let mut vm = use_signal(Vm::default);
    let mut toast = use_signal(|| None::<String>);

    // 最新内核快照 → 唯一的渲染真相来源(同旧壳 main.rs 同款 `use_future`
    // 订阅 `watch::Receiver` 的写法)。
    use_future({
        let kernel = kernel.clone();
        move || {
            let mut rx = kernel.vm();
            async move {
                let first = rx.borrow().clone();
                vm.set(first);
                while rx.changed().await.is_ok() {
                    let next = rx.borrow().clone();
                    vm.set(next);
                }
            }
        }
    });

    // 转瞬即逝的提示——目前只有错误会弹 toast(`UiNote::Ok` 暂不需要打
    // 扰用户,壳里没有任何耗时到需要"成功了"提示的写操作)。
    use_future({
        let kernel = kernel.clone();
        move || {
            let mut rx = kernel.notes();
            async move {
                loop {
                    match rx.recv().await {
                        Ok(UiNote::Error(e)) => toast.set(Some(e)),
                        // 命令真的成功了——把 `Event` 的 `Debug` 文本带一
                        // 句进 toast,给人一个"刚才那次操作确实发生了"的
                        // 确认(模块文档「壳里…提示出」通道的字段本身就
                        // 是给这里用的,不是摆设)。
                        Ok(UiNote::Ok(msg)) => toast.set(Some(format!("✓ {msg}"))),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    let current = vm();
    rsx! {
        GlobalChrome {}
        div {
            style: "display:flex;flex-direction:column;height:100vh;background:{theme::PAPER};font-family:{theme::SANS};color:{theme::INK};",
            {top_bar(kernel.clone(), current.clone())}
            div {
                style: "flex:1;min-height:0;overflow:auto;padding:20px 28px;",
                if !current.ready {
                    div { style: "color:{theme::INK_3};padding:40px;text-align:center;", "正在打开本地工作台…" }
                } else if let Some(msg) = current.fatal.clone() {
                    div { style: "color:{theme::signal_color(bw_core::Signal::Red)};padding:24px;", "无法渲染:{msg}" }
                } else {
                    match current.panel {
                        Panel::Hex => {
                            // next 切片七D(design-s7-real-project.md §1
                            // 「壳的活卡加『开工』『取消』」)——同上面
                            // `Panel::Attention` 分支的既有分工:这两个闭
                            // 包是唯一把 `ShellCommand::RunIssue`/
                            // `ShellCommand::CancelRun` 摆上台面的地方,复
                            // 核起来只用看这一处。
                            let kernel_for_run = kernel.clone();
                            let kernel_for_cancel = kernel.clone();
                            rsx! {
                                screens::hex::HexScreen {
                                    vm: current.clone(),
                                    on_run_issue: move |id: bw_core::IssueId| {
                                        kernel_for_run.send(ShellCommand::RunIssue(id));
                                    },
                                    on_cancel_run: move |run: bw_core::RunId| {
                                        kernel_for_cancel.send(ShellCommand::CancelRun(run));
                                    },
                                }
                            }
                        }
                        Panel::Attention => {
                            // 「完成永远人点」的唯一发令点(硬约束)——这
                            // 个闭包只发 `Command::TransitionIssue { to:
                            // Done }`,没有第二条路径。`kernel.clone()` 是
                            // 廉价克隆(两个 channel 把手 + 一个 broadcast
                            // sender),每次渲染重建这个闭包不是问题。
                            let kernel_for_complete = kernel.clone();
                            rsx! {
                                screens::attention::AttentionScreen {
                                    vm: current.clone(),
                                    on_complete: move |id: bw_core::IssueId| {
                                        kernel_for_complete.send(ShellCommand::Dispatch(
                                            bw_app::Command::TransitionIssue {
                                                issue: id,
                                                to: bw_core::IssueStatus::Done,
                                            },
                                        ));
                                    },
                                }
                            }
                        }
                    }
                }
            }
            if let Some(msg) = toast() {
                div {
                    style: "position:fixed;bottom:18px;left:50%;transform:translateX(-50%);background:{theme::ALERT_DEEP};color:#FFF;padding:10px 18px;border-radius:8px;font-size:13px;max-width:70vw;",
                    onclick: move |_| toast.set(None),
                    "{msg}"
                }
            }
        }
    }
}

/// **不是** `#[component]`——`Kernel` 内含 tokio channel 把手,没有(也不
/// 该造一个假的)`PartialEq`,dioxus 的 `#[component]` 宏要求每个 prop 都
/// `PartialEq`(变更检测)。同旧壳 `screens/component_detail.rs`
/// `adopt_button` 的既有先例:普通函数,返回 `Element`,在 `rsx!` 里用
/// `{top_bar(...)}` 当一个表达式节点调用,不经过 props 机制。
fn top_bar(kernel: Kernel, vm: Vm) -> Element {
    let (north_star_text, dot_color) = match vm.hex.as_ref().map(|h| &h.north_star) {
        Some(bw_app::view::hex::NorthStarView::Defined(card)) => {
            (card.name.clone(), theme::signal_color(card.signal))
        }
        Some(bw_app::view::hex::NorthStarView::Undefined) => (
            "北极星尚未定稿".to_string(),
            theme::signal_color(bw_core::Signal::Unknown),
        ),
        None => (String::new(), theme::signal_color(bw_core::Signal::Unknown)),
    };
    let selected_value = vm
        .selected
        .map(|p| p.uuid().to_string())
        .unwrap_or_default();

    rsx! {
        div {
            style: "display:flex;align-items:center;gap:14px;padding:10px 20px;background:{theme::RAIL_BG};border-bottom:1px solid {theme::BORDER};flex:none;",
            div { style: "font-family:{theme::SERIF};font-weight:700;font-size:15px;color:{theme::CLAY};", "BW" }
            select {
                value: "{selected_value}",
                style: "font-size:13px;padding:4px 8px;border-radius:6px;border:1px solid {theme::BORDER_DEEP};background:{theme::CARD};",
                onchange: {
                    let kernel = kernel.clone();
                    move |evt: Event<FormData>| {
                        if let Ok(uuid) = uuid::Uuid::parse_str(&evt.value()) {
                            kernel.send(ShellCommand::SelectProject(bw_core::ProjectId::from_uuid(uuid)));
                        }
                    }
                },
                option { value: "", disabled: true, "选择项目…" }
                for p in vm.projects.iter() {
                    option { value: "{p.id.uuid()}", selected: p.id.uuid().to_string() == selected_value, "{p.name}" }
                }
            }
            if !north_star_text.is_empty() {
                div {
                    style: "display:flex;align-items:center;gap:6px;",
                    span { style: "{theme::dot(dot_color, 9)}" }
                    span { style: "font-size:13px;color:{theme::INK_2};white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:360px;", "{north_star_text}" }
                }
            }
            div { style: "flex:1;" }
            button {
                style: panel_btn_style(vm.panel == Panel::Hex),
                onclick: {
                    let kernel = kernel.clone();
                    move |_| kernel.send(ShellCommand::SetPanel(Panel::Hex))
                },
                "六段总控"
            }
            button {
                style: panel_btn_style(vm.panel == Panel::Attention),
                onclick: {
                    let kernel = kernel.clone();
                    move |_| kernel.send(ShellCommand::SetPanel(Panel::Attention))
                },
                "待人处理"
            }
            button {
                style: panel_btn_style(false),
                onclick: move |_| kernel.send(ShellCommand::Refresh),
                "刷新"
            }
        }
    }
}

fn panel_btn_style(active: bool) -> String {
    if active {
        format!(
            "border:none;border-radius:7px;padding:6px 14px;font-size:12px;font-weight:600;background:{};color:#FFF;",
            theme::CLAY
        )
    } else {
        format!(
            "border:1px solid {};border-radius:7px;padding:6px 14px;font-size:12px;background:{};color:{};",
            theme::BORDER_DEEP,
            theme::CARD,
            theme::INK_2
        )
    }
}
