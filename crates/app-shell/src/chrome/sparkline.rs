//! 走势小图 —— 内联 SVG,不引任何图表库。
//!
//! **几何取舍照搬 V3 那版已经跑顺的**(`crates/ui` 里那两个纯函数),但代码是
//! 这边重写的一份:`ui` 那个 crate 挂在 `bw-core` 上,而 V4 正在断掉那条依赖,
//! 搬过来等于把依赖又接回去。
//!
//! 抄过来的四条取舍,每条都有它的道理:
//!
//! - **x 轴按点均分**,不按真实日期比例 —— 每周一个点,间距本来就该相等;
//!   只有一个点时居中,不贴边。
//! - **y 轴走 min-max(留 8% 余量),不从 0 起** —— 从 0 起会把「142→148」这种
//!   真实波动压成一条平线。整条全平时人为撑开 ±1,让线落在中间而不是贴着框。
//! - **空数据整块不画**,显示一句「尚无观测」。画一条 0 高的线等于宣称「这几周
//!   都是 0」,那是编。
//! - **采不到的点断开,绝不当 0 连过去** —— 这是和 V3 唯一不同的一条:V3 上游
//!   用「补前值」把空档填平,读的人分不出「没变」和「没数」。这里宁可断。

use dioxus::prelude::*;

/// 一条走势线。
pub struct Series {
    /// 图上方那行小标题。
    pub label: String,
    /// `(x 轴标签, 值)`,旧的在前。`None` = 那一格没采到,画的时候断开。
    pub points: Vec<(String, Option<f64>)>,
    /// CSS 变量名,如 `var(--clay)`。
    pub color: &'static str,
}

// 画布尺寸与内边距。左边留给 y 刻度文字,下边留给 x 标签,上边留给点上的数值。
const W: f64 = 232.0;
const H: f64 = 104.0;
const PAD_L: f64 = 30.0;
const PAD_R: f64 = 8.0;
const PAD_T: f64 = 16.0;
const PAD_B: f64 = 18.0;

pub fn trend_chart(s: &Series) -> Element {
    let plot_w = W - PAD_L - PAD_R;
    let plot_h = H - PAD_T - PAD_B;
    let n = s.points.len();
    let vals: Vec<Option<f64>> = s.points.iter().map(|(_, v)| *v).collect();
    let known: Vec<f64> = vals.iter().flatten().copied().collect();

    // 一个点都没采到 —— 不画,说实话。
    if known.is_empty() {
        return rsx! {
            div { class: "trend-box",
                div { class: "trend-label", "{s.label}" }
                div { class: "detail-empty", style: "padding:14px 0;font-size:12px;",
                    "尚无观测"
                }
            }
        };
    }

    let min = known.iter().copied().fold(f64::INFINITY, f64::min);
    let max = known.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // 全平的序列人为撑开,免得线贴着边框看不见。
    let (y_min, y_max) = if (max - min).abs() <= f64::EPSILON {
        (min - 1.0, min + 1.0)
    } else {
        let pad = (max - min) * 0.08;
        (min - pad, max + pad)
    };

    let x_at = |i: usize| -> f64 {
        if n <= 1 {
            PAD_L + plot_w / 2.0
        } else {
            PAD_L + (i as f64 / (n - 1) as f64) * plot_w
        }
    };
    let y_at = |v: f64| -> f64 { PAD_T + plot_h - (v - y_min) / (y_max - y_min) * plot_h };

    // 采不到的点把线断开:每一段连续的已知值各起一个 `M`。
    let mut d = String::new();
    let mut pen_down = false;
    for (i, v) in vals.iter().enumerate() {
        match v {
            None => pen_down = false,
            Some(v) => {
                let (x, y) = (x_at(i), y_at(*v));
                if pen_down {
                    d.push_str(&format!(" L{x:.1} {y:.1}"));
                } else {
                    d.push_str(&format!(" M{x:.1} {y:.1}"));
                    pen_down = true;
                }
            }
        }
    }

    let plot_bottom = PAD_T + plot_h;
    let color = s.color;
    rsx! {
        div { class: "trend-box",
            div { class: "trend-label", "{s.label}" }
            svg {
                width: "100%",
                height: "{H}",
                view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "xMidYMid meet",
                // 上下两条刻度线 + 数值,够看出量程了;四个点的小图不必画满格。
                for (v, y) in [(y_max, y_at(y_max)), (y_min, y_at(y_min))] {
                    g { key: "tick{v}",
                        line {
                            x1: "{PAD_L}", y1: "{y:.1}", x2: "{W - PAD_R}", y2: "{y:.1}",
                            stroke: "var(--border)", stroke_width: "1", stroke_dasharray: "3 3",
                        }
                        text {
                            x: "{PAD_L - 5.0}", y: "{y + 3.5:.1}", text_anchor: "end",
                            style: "font-family:var(--mono);font-size:9px;fill:var(--ink-3);",
                            "{fmt_tick(v)}"
                        }
                    }
                }
                path {
                    d: "{d}", fill: "none", stroke: "{color}",
                    stroke_width: "2", stroke_linejoin: "round", stroke_linecap: "round",
                }
                for (i, (label, v)) in s.points.iter().enumerate() {
                    g { key: "p{i}",
                        if let Some(v) = v {
                            circle {
                                cx: "{x_at(i):.1}", cy: "{y_at(*v):.1}", r: "2.8",
                                fill: "var(--paper)", stroke: "{color}", stroke_width: "1.6",
                            }
                            text {
                                x: "{x_at(i):.1}", y: "{y_at(*v) - 7.0:.1}", text_anchor: "middle",
                                style: "font-family:var(--mono);font-size:9px;font-weight:600;fill:{color};",
                                "{fmt_tick(*v)}"
                            }
                        }
                        text {
                            x: "{x_at(i):.1}", y: "{plot_bottom + 12.0:.1}", text_anchor: "middle",
                            style: "font-family:var(--mono);font-size:9px;fill:var(--ink-3);",
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// 接近整数就显示整数,否则留一位小数 —— 计数类指标不该显示 `3.0`。
fn fmt_tick(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}
