//! Role playbooks — the five stage roles as *executable* methodology.
//!
//! `StageKind`'s static metadata (core question / method loop / DoD / anti-
//! patterns) tells a human what each stage is. A [`Playbook`] goes one step
//! further: per method-loop phase, a **real instruction** a real executor
//! (e.g. `ClaudeCliExecutor`) can act on inside a project workspace —
//! producing files, commits and test runs, not just prose.
//!
//! Same honesty rules as everything else in this crate:
//! - the playbook is *methodology* (generic, versioned in code), never
//!   per-project content — project specifics enter only through
//!   [`PlaybookCtx`] template variables;
//! - every instruction demands real, verifiable artifacts (files in the
//!   workspace, real command output quoted into docs) and explicitly forbids
//!   invented numbers;
//! - phases that cannot honestly be parallel today (e.g. Build's
//!   "Agent 并行实现") say so instead of pretending.
//!
//! Rendering is a plain `{var}` substitution — no template engine, no IO.

use crate::model::StageKind;

/// Project context injected into a playbook's `{var}` slots. All fields are
/// real data captured by the creation flow / operating state — the playbook
/// never invents any of them.
#[derive(Clone, Debug, Default)]
pub struct PlaybookCtx {
    pub project_name: String,
    pub project_kind: String,
    /// The free-text brief from the creation flow (意图 card).
    pub project_desc: String,
    /// 对标竞品 (creation flow 快答).
    pub benchmark: String,
    /// 三个月成功标准 (creation flow 快答).
    pub opportunity: String,
    pub north_star: String,
    pub ns_def: String,
    /// The *previous* stage's real handoff note — the baton this stage
    /// received. Empty on the very first stage of a cycle.
    pub handoff_note: String,
    /// Free-text hint about the workspace (e.g. "全新空目录，请自行初始化
    /// git 与项目骨架"). Real state, described by the caller.
    pub workspace_hint: String,
}

/// Replace every `{var}` slot in `template` from `ctx`. Unknown slots are left
/// verbatim (they are visible in output — an honest bug signal, not a crash).
pub fn render(template: &str, ctx: &PlaybookCtx) -> String {
    let empty = |s: &str, fallback: &str| -> String {
        if s.trim().is_empty() {
            fallback.to_string()
        } else {
            s.to_string()
        }
    };
    template
        .replace("{project_name}", &ctx.project_name)
        .replace("{project_kind}", &ctx.project_kind)
        .replace("{project_desc}", &empty(&ctx.project_desc, "（未填写）"))
        .replace("{benchmark}", &empty(&ctx.benchmark, "（未填写）"))
        .replace("{opportunity}", &empty(&ctx.opportunity, "（未填写）"))
        .replace("{north_star}", &empty(&ctx.north_star, "（尚未定稿）"))
        .replace("{ns_def}", &empty(&ctx.ns_def, "（尚未定稿）"))
        .replace(
            "{handoff_note}",
            &empty(&ctx.handoff_note, "（本阶段是第一棒，无上一棒交接词）"),
        )
        .replace(
            "{workspace_hint}",
            &empty(&ctx.workspace_hint, "（调用方未描述工作区状态）"),
        )
}

/// The role header prepended to every phase instruction of a stage — the
/// stage's own static methodology, phrased as a working identity.
pub fn role_preamble(kind: StageKind) -> String {
    format!(
        "你是{role}（方法论：{methodology} · 心法：{seek}）。\n\
         你所在阶段的核心问题：{question}\n\
         本阶段反模式（务必避免）：{anti}\n\
         \n\
         ## 项目上下文（全部为真实录入，非编造）\n\
         - 项目：{{project_name}}（{{project_kind}}）\n\
         - Brief：{{project_desc}}\n\
         - 对标：{{benchmark}}\n\
         - 三个月成功标准：{{opportunity}}\n\
         - 北极星：{{north_star}} —— {{ns_def}}\n\
         - 上一棒交接词：{{handoff_note}}\n\
         - 工作区状态：{{workspace_hint}}\n\
         \n\
         ## 诚实约束（不可妥协）\n\
         1. 绝不编造数据、引用或运行结果；引用命令输出必须真实执行过。\n\
         2. 无法验证的判断要明确标注「未验证」。\n\
         3. 一切产出落为工作区内的真实文件；完成后用一段话总结你真实做了什么。\n",
        role = kind.role(),
        methodology = kind.methodology(),
        seek = kind.seek(),
        question = kind.core_question(),
        anti = kind.anti_patterns(),
    )
}

/// Per method-loop phase instructions for one stage. Index-aligned with
/// [`StageKind::method_loop`] — a unit test pins the two in lockstep.
pub fn phase_instructions(kind: StageKind) -> &'static [&'static str] {
    match kind {
        StageKind::Prototype => &[
            // 证据
            "本 phase：证据。调研这个需求的现状与竞品证据，在工作区创建 docs/evidence.md：\
             （1）该需求要解决的问题，现有解法/竞品有哪些（基于你已有知识，无法联网核实的注明\
             「未核实」）；（2）目标用户与真实使用场景；（3）现状的具体痛点。只写你能站得住的\
             内容，禁止编造统计数字。",
            // 洞察
            "本 phase：洞察。读工作区 docs/evidence.md，把证据提炼为至多 3 条关键洞察，写入 \
             docs/insights.md；每条洞察注明它来自哪条证据（引用文件小节）。没有证据支撑的洞察\
             不许写。",
            // 假设
            "本 phase：假设。读 docs/insights.md，压缩成一句话核心假设 + 可证伪的 DoD，写入 \
             docs/hypothesis.md，格式：## 假设 / ## DoD（完成定义，必须可客观判定）/ ## 证伪条件\
             （什么样的真实结果会推翻这个假设）。",
            // 原型
            "本 phase：原型。按 docs/hypothesis.md 的假设造最小可跑原型：若工作区还没有项目\
             骨架，先初始化（Rust 项目用 cargo init）；实现最薄的核心路径，能真实跑通一个最小\
             例子即可。原型阶段不追求代码质量（那是构建段的事），能证假设就行。完成后确保 \
             `cargo run`（或等价命令）真实可跑。",
            // 验证
            "本 phase：验证。真实运行原型（cargo run / cargo test），把真实运行输出与 \
             docs/hypothesis.md 的 DoD 逐条对照，结论写入 docs/validation.md：假设成立/不成立/\
             部分成立 + 真实运行输出摘录（原样粘贴，不美化）。若不成立，如实写明——不成立也是\
             合法结论。最后 `git add -A && git commit -m \"prototype: 假设验证，结论见 \
             docs/validation.md\"`（原型也要留下真实的 git 证据）。",
        ],
        StageKind::Build => &[
            // 规格 Spec
            "本 phase：规格 Spec。基于原型段验证过的假设（读 docs/hypothesis.md 与 \
             docs/validation.md），写 docs/SPEC.md：完整功能规格 —— 输入/输出、CLI 参数、错误\
             处理、边界情况、验收标准。每条验收标准必须可测试（能翻译成一个测试用例）。",
            // 任务分解
            "本 phase：任务分解。把 docs/SPEC.md 分解为有序的实现任务清单，写入 docs/TASKS.md：\
             每个任务一行 + 它对应的验收标准编号。任务粒度以「一次提交能完成」为宜。",
            // Agent 并行实现
            "本 phase：实现。按 docs/TASKS.md 逐项实现完整功能（生产质量：错误处理、边界情况\
             全覆盖），并为每条验收标准补齐对应的单元/集成测试。完成后 `cargo test` 必须全绿，\
             如实在 docs/TASKS.md 里勾掉完成项。（方法论原文是「Agent 并行实现」——当前执行器\
             是单 agent 顺序执行，如实按顺序做，不假装并行。）",
            // 评审合入 · CI 门禁
            "本 phase：评审合入 · CI 门禁。以评审者身份自查：对照 docs/SPEC.md 验收标准逐条\
             核对，结果写入 docs/REVIEW.md（每条：通过/未通过 + 依据）；真实跑 `cargo fmt --check`、\
             `cargo clippy -- -D warnings`、`cargo test`，把真实结果（含失败）记进 REVIEW.md；\
             全绿后 `git add -A && git commit -m \"build: 按 SPEC 完成实现与测试\"`。",
        ],
        StageKind::Optimize => &[
            // 基线测量
            "本 phase：基线测量。真实测量当前基线并写入 docs/baseline.md：cargo test 通过数、\
             cargo clippy -- -D warnings 的警告/错误数、`wc -l src/**` 代码行数、cargo build \
             真实耗时。全部数字必须来自你真实执行的命令输出（原样摘录），禁止估算。",
            // 瓶颈定位
            "本 phase：瓶颈定位。通读代码，找出：重复逻辑、过度复杂的实现、无用依赖、性能可疑点、\
             可删减项。按影响排序写入 docs/bottlenecks.md，每条附具体代码位置（文件:行）。\
             记住优化师的戒律：只优化不删减是警报——优先找可删的。",
            // 优化 / 删减
            "本 phase：优化/删减。对 docs/bottlenecks.md 的 top 项做小步等价重构与删减：\
             绝不加新功能（那是回退到构建段），每步保持 `cargo test` 全绿。删掉的代码行数\
             也是成果。",
            // 回归验证
            "本 phase：回归验证。重跑基线测量（与 docs/baseline.md 完全相同的命令），把前后\
             对比写入 docs/regression.md：各指标 delta（行数、警告数、测试数、耗时）。确认无\
             回归后 `git add -A && git commit -m \"optimize: 基线对比见 docs/regression.md\"`。",
        ],
        StageKind::Growth => &[
            // 漏斗诊断
            "本 phase：漏斗诊断。以一个从未见过这个项目的新用户视角，真实走一遍「发现 → 安装 → \
             首次使用 → 再次使用」漏斗，把每一步的真实摩擦点写入 docs/funnel.md。「安装」用 \
             `cargo build --release` + 直接运行 target/release 下的二进制来真实代理（**不要** \
             `cargo install` 污染本机 PATH）。不臆想用户，只记录你真实操作中遇到的障碍。",
            // 实验设计
            "本 phase：实验设计。针对 docs/funnel.md 里最大的摩擦点，设计一个最小改动实验，\
             写入 docs/growth-experiment.md：改动内容 / 预期效果 / 衡量方式（前后对照什么真实\
             输出）。（方法论原文是 A/B 实验——本项目当前没有真实流量，如实采用「前后对照」\
             而非假装有 A/B 流量。）",
            // A/B 上线
            "本 phase：上线改动。实施 docs/growth-experiment.md 的改动（典型：README.md 快速\
             开始、examples/ 示例、--help 文案、安装一行命令），完成后以新用户视角真实重走一遍\
             首用路径验证可用。",
            // 放大或废弃
            "本 phase：放大或废弃。按 docs/growth-experiment.md 的衡量方式做前后对照（真实输出\
             摘录），结论写入 docs/growth-verdict.md：放大/保留/废弃 + 依据。然后 \
             `git add -A && git commit -m \"growth: 实验结论见 docs/growth-verdict.md\"`。",
        ],
        StageKind::Ops => &[
            // SLO / 错误预算
            "本 phase：SLO/错误预算。为这个项目定义诚实可测的质量红线，写入 docs/SLO.md：\
             例如「坏输入必须友好报错，绝不 panic」「cargo test 通过率 100%」「clippy 0 警告」。\
             每条红线当场真实测量当前值并记录（真实命令输出摘录）。",
            // 监控告警
            "本 phase：监控告警。写 scripts/healthcheck.sh：一键真实跑 fmt --check / clippy \
             -D warnings / test + 一次冒烟运行，任何失败以非零码退出。`chmod +x` 后真实执行\
             一遍，把完整输出摘录到 docs/healthcheck-run.md。",
            // 事故响应
            "本 phase：事故响应演练。对工具做破坏性输入测试：不存在的文件、坏参数、空输入、\
             超长输入等，逐个真实执行并把行为记录到 docs/incident-drill.md。发现 panic 或不\
             友好报错即当场修复，修复后重测并记录。",
            // 复盘回灌
            "本 phase：复盘回灌。写 docs/retro.md：本圈五阶段各自真实完成了什么（引用真实存在\
             的文件与 git log 里的真实提交号）、最大的教训、以及回流给下一圈原型段的 1-2 个新\
             假设。所有引用必须真实存在（写之前用 ls / git log 核实）。完成后 \
             `git add -A && git commit -m \"ops: 复盘回灌，线闭成环\"`。",
        ],
    }
}

/// Render the full per-phase prompt list for one stage: role preamble +
/// project context + the phase's own instruction. Index-aligned with
/// [`StageKind::method_loop`].
pub fn rendered_phase_prompts(kind: StageKind, ctx: &PlaybookCtx) -> Vec<String> {
    let preamble = render(&role_preamble(kind), ctx);
    phase_instructions(kind)
        .iter()
        .map(|instr| format!("{preamble}\n## 当前任务\n{}", render(instr, ctx)))
        .collect()
}

/// The stage's shared prompt (used as the workflow-level `prompt`, and the
/// fallback for any phase without its own prompt).
pub fn stage_prompt(kind: StageKind, ctx: &PlaybookCtx) -> String {
    format!(
        "{}\n## 当前任务\n按「{}」方法循环推进本阶段：{}",
        render(&role_preamble(kind), ctx),
        kind.methodology(),
        kind.method_loop().join(" → "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stage's playbook must stay in lockstep with its method loop —
    /// one real instruction per phase, no more, no fewer.
    #[test]
    fn playbook_phases_align_with_method_loop() {
        for kind in StageKind::ALL {
            assert_eq!(
                phase_instructions(kind).len(),
                kind.method_loop().len(),
                "{kind:?} 的剧本 phase 数必须与方法循环一致"
            );
        }
    }

    #[test]
    fn render_substitutes_real_ctx_and_marks_empty_fields() {
        let ctx = PlaybookCtx {
            project_name: "linkcheck-md".into(),
            project_kind: "CLI 工具".into(),
            project_desc: "检查 Markdown 死链".into(),
            ..Default::default()
        };
        let out = render(
            "项目 {project_name}（{project_kind}）：{project_desc}；对标：{benchmark}",
            &ctx,
        );
        assert!(out.contains("linkcheck-md"));
        assert!(out.contains("CLI 工具"));
        assert!(out.contains("检查 Markdown 死链"));
        // Empty fields render as an explicit "not filled" marker, never as
        // silently-empty text that could read like fabricated content.
        assert!(out.contains("（未填写）"));
    }

    #[test]
    fn rendered_prompts_carry_role_and_honesty_rules() {
        let ctx = PlaybookCtx {
            project_name: "demo".into(),
            ..Default::default()
        };
        for kind in StageKind::ALL {
            let prompts = rendered_phase_prompts(kind, &ctx);
            assert_eq!(prompts.len(), kind.method_loop().len());
            for p in &prompts {
                assert!(
                    p.contains(kind.role_short()),
                    "{kind:?} 每个 phase 都带角色身份"
                );
                assert!(p.contains("绝不编造"), "{kind:?} 每个 phase 都带诚实约束");
                assert!(p.contains("demo"), "{kind:?} 每个 phase 都带项目上下文");
            }
        }
    }

    /// No `{var}` slot may survive rendering with a fully-populated ctx —
    /// a leftover brace means a typo'd slot name.
    #[test]
    fn no_unresolved_slots_with_full_ctx() {
        let ctx = PlaybookCtx {
            project_name: "n".into(),
            project_kind: "k".into(),
            project_desc: "d".into(),
            benchmark: "b".into(),
            opportunity: "o".into(),
            north_star: "ns".into(),
            ns_def: "nsd".into(),
            handoff_note: "h".into(),
            workspace_hint: "w".into(),
        };
        for kind in StageKind::ALL {
            for p in rendered_phase_prompts(kind, &ctx) {
                assert!(
                    !p.contains("{project_") && !p.contains("{north_") && !p.contains("{handoff_"),
                    "{kind:?} 存在未替换的模板槽位: {p}"
                );
            }
        }
    }
}
