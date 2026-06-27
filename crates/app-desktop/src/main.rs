//! **P0 Dioxus ramp — "hello signals" (throwaway).**
//!
//! Purpose: climb the first-time Dioxus 0.7 learning curve (plan `00 §8 risk-1`)
//! on a throwaway app before the real shell. It exercises exactly the primitives
//! P2 will lean on — `use_signal`, `rsx!`, event handlers, derived display — and
//! it drives them through the *real kernel*: signal swatches use
//! [`ui::signal_color`], and the rolled-up "project" swatch is a live
//! [`bw_core::derive::reduce_worst_of`] (the L4/L6 worst-of), proving the
//! kernel→UI seam end to end.
//!
//! Run with `dx serve --package app-desktop` (or `cargo run -p app-desktop`).
//! This file is expected to be deleted once the real shell exists.

use bw_core::derive::reduce_worst_of;
use bw_core::Signal;
use dioxus::prelude::*;
use ui::signal_color;

const PAPER: &str = "#EFEBE2";
const INK: &str = "#23211C";
const CLAY: &str = "#C5654A";

fn main() {
    dioxus::launch(App);
}

/// Cycle a signal Green → Amber → Red → Unknown → Green (ramp interaction).
fn next(s: Signal) -> Signal {
    match s {
        Signal::Green => Signal::Amber,
        Signal::Amber => Signal::Red,
        Signal::Red => Signal::Unknown,
        Signal::Unknown => Signal::Green,
    }
}

fn label(s: Signal) -> &'static str {
    match s {
        Signal::Green => "Green",
        Signal::Amber => "Amber",
        Signal::Red => "Red",
        Signal::Unknown => "Unknown",
    }
}

#[component]
fn App() -> Element {
    // Three child "stage" signals — click to cycle each.
    let mut a = use_signal(|| Signal::Green);
    let mut b = use_signal(|| Signal::Green);
    let mut c = use_signal(|| Signal::Green);

    // L6 roll-up, recomputed from the kernel on every render.
    let rolled = reduce_worst_of([a(), b(), c()]).into_inner();

    rsx! {
        div {
            style: "min-height:100vh;background:{PAPER};color:{INK};font-family:system-ui,sans-serif;padding:40px;",
            h1 {
                style: "font-weight:600;margin:0 0 4px;",
                "Builders' Workbench — Dioxus ramp"
            }
            p {
                style: "color:#57534A;margin:0 0 28px;",
                "Click a stage to cycle its signal. The project rolls up via the real "
                code { "reduce_worst_of" }
                " (any red → red; else any amber → amber; else unknown-without-green → unknown)."
            }

            div {
                style: "display:flex;gap:16px;margin-bottom:32px;",
                Swatch { name: "Stage A", sig: a(), onclick: move |_| a.set(next(a())) }
                Swatch { name: "Stage B", sig: b(), onclick: move |_| b.set(next(b())) }
                Swatch { name: "Stage C", sig: c(), onclick: move |_| c.set(next(c())) }
            }

            div {
                style: "padding:20px;border-radius:12px;background:#FBFAF6;border:1px solid #E2DCCF;box-shadow:0 8px 26px rgba(35,33,28,.08);display:inline-block;",
                div { style: "font-size:13px;color:#8C867A;margin-bottom:8px;", "PROJECT (L6 derived)" }
                div {
                    style: "display:flex;align-items:center;gap:10px;",
                    span {
                        style: "width:14px;height:14px;border-radius:50%;background:{signal_color(rolled)};display:inline-block;"
                    }
                    span { style: "font-weight:500;", "{label(rolled)}" }
                }
            }
        }
    }
}

#[component]
fn Swatch(name: &'static str, sig: Signal, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            onclick: move |e| onclick.call(e),
            style: "cursor:pointer;padding:18px 22px;border-radius:10px;background:#F4F0E7;border:1px solid #DBD4C5;text-align:left;min-width:130px;",
            div { style: "font-size:13px;color:#8C867A;margin-bottom:10px;", "{name}" }
            div {
                style: "display:flex;align-items:center;gap:8px;",
                span {
                    style: "width:12px;height:12px;border-radius:50%;background:{signal_color(sig)};display:inline-block;border:1px solid rgba(0,0,0,.08);"
                }
                span { style: "color:{CLAY};font-weight:500;", "{label(sig)}" }
            }
        }
    }
}
