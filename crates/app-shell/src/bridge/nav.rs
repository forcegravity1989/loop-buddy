//! 界面内部的导航类型:去哪个入口、去哪个顶层屏、指南翻到哪一章。
//!
//! **这些都不经内核**。切入口、开抽屉、从项目墙跳到接入屏,全是纯本机导航,
//! 不改任何数据,所以它们不是 `Req`,而是各屏都拿得到的共享信号。放在桥这个
//! 模块下是因为深链(`BW_PANEL` / `BW_VIEW`)要用同一套枚举解析。

/// 深链要跳到哪。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Overview,
    Plan,
    Session,
    Notify,
    Config,
    Kb,
}

impl Panel {
    pub fn parse(s: &str) -> Option<Panel> {
        Some(match s {
            "overview" => Panel::Overview,
            "plan" => Panel::Plan,
            "session" => Panel::Session,
            "notify" => Panel::Notify,
            "config" => Panel::Config,
            "kb" => Panel::Kb,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Panel::Overview => "总览",
            Panel::Plan => "计划",
            Panel::Session => "会话",
            Panel::Notify => "通知",
            Panel::Config => "配置",
            Panel::Kb => "知识库",
        }
    }

    pub const ALL: [Panel; 6] = [
        Panel::Overview,
        Panel::Plan,
        Panel::Session,
        Panel::Notify,
        Panel::Config,
        Panel::Kb,
    ];
}

/// 「跳到另一个入口」的口子。六入口之间切换是**纯本机导航**,不经内核 ——
/// 所以它不是一条 `Req`,而是一个各屏都拿得到的信号。屏里想跳(总览的
/// 「去计划 →」、通知里点一条跳会话)就 `use_context::<PanelNav>()`。
#[derive(Clone, Copy)]
pub struct PanelNav(pub dioxus::prelude::Signal<Panel>);

impl PanelNav {
    pub fn go(mut self, p: Panel) {
        use dioxus::prelude::WritableExt;
        self.0.set(p);
    }
}

/// 指南抽屉开在哪一章。`None` = 收着。和 [`PanelNav`] 一样是纯本机导航,
/// 所以也走 context 不走 `Req` —— 项目墙那条「怎么处理 →」按下去要能直接把
/// 抽屉翻到环境那一章,抽屉本身却挂在外壳上,两边只能靠这个共享信号说话。
#[derive(Clone, Copy)]
pub struct GuideNav(pub dioxus::prelude::Signal<Option<&'static str>>);

impl GuideNav {
    pub fn open(mut self, chapter: &'static str) {
        use dioxus::prelude::WritableExt;
        self.0.set(Some(chapter));
    }
    pub fn close(mut self) {
        use dioxus::prelude::WritableExt;
        self.0.set(None);
    }
}

/// 顶层三屏里不依赖「打开某个项目」的那两个。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopView {
    Onboard,
    Settings,
}

impl TopView {
    pub fn parse(s: &str) -> Option<TopView> {
        Some(match s {
            "onboard" => TopView::Onboard,
            "settings" => TopView::Settings,
            _ => return None,
        })
    }
}
