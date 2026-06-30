//! **Builders' Workbench — desktop shell (P2-A).**
//!
//! The real Dioxus shell, replacing the P0 throwaway. [`Root`] installs the
//! global look (warm-paper CSS + font stack), stands up the Event→Signal bridge
//! ([`bridge::use_app_host`]) which owns the [`bw_app::App`] inside a coroutine
//! and shares a [`bridge::ViewModel`] + [`bridge::CommandBus`] via context, then
//! renders the [`shell::Shell`] (64px icon rail + view router).
//!
//! Architecture rule (non-negotiable #1): `dioxus`/`wry` live ONLY in this crate.
//! The kernel (bw-core/bw-engine/bw-store/bw-app/ui) never sees a UI dep — the
//! shell drives them purely command-in / snapshot-out through the bridge.

mod bridge;
mod icons;
mod screens;
mod shell;
mod theme;

use dioxus::prelude::*;

use crate::shell::Shell;

fn main() {
    dioxus::launch(Root);
}

/// Global stylesheet (plan `01 §6`): warm-paper body, selection tint, and the
/// custom scrollbar. Fonts are a CSS stack with CJK fallbacks — see [`Root`].
///
/// TODO(P2): bundle real Noto Serif SC / Noto Sans SC / JetBrains Mono via
/// `asset!()` from `assets/fonts/` and add the matching `@font-face` blocks. The
/// stack below is the graceful fallback until those binaries land; it keeps the
/// shell shippable without blocking on font files.
const GLOBAL_CSS: &str = "
html,body{background:#EFEBE2;margin:0;padding:0;}
body{
  font-family:'Noto Sans SC','PingFang SC',system-ui,sans-serif;
  color:#23211C;
  -webkit-font-smoothing:antialiased;
  text-rendering:optimizeLegibility;
}
*{box-sizing:border-box;}
::selection{background:#E7CFC4;}
::-webkit-scrollbar{width:12px;height:12px;}
::-webkit-scrollbar-thumb{
  background:#D8D1C2;border-radius:8px;border:3px solid #EFEBE2;
}
::-webkit-scrollbar-track{background:transparent;}
";

/// The application root: inject global CSS, host the App bridge, render the shell.
#[component]
fn Root() -> Element {
    // Stand up the Event→Signal bridge. This OWNS the `App` inside a coroutine
    // and provides `Signal<ViewModel>` + `CommandBus` via context for every
    // child screen. (Return value also provided as context; ignored here.)
    let _vm = bridge::use_app_host();

    rsx! {
        // Global, theme-wide styles injected into <head>.
        document::Style { {GLOBAL_CSS} }
        Shell {}
    }
}
