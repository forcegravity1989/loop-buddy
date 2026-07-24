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

/// T8 (plan/12 §4): real per-phase `role` + (Static-only) fixed
/// `reject_to_phase` for one stage's method loop — index-aligned with
/// [`StageKind::method_loop`]/[`phase_instructions`]. Judged phase by phase
/// against what [`phase_instructions`] actually has that phase do, not
/// stamped mechanically:
///
/// - **Prototype** (证据/洞察/假设/原型/验证): 证据/洞察 are research inputs
///   (`Neutral`); 假设/原型 each produce a deliverable (`Generator`); 验证 is
///   the real-usage judgment gate (`Evaluator`) — a failed validation is a
///   bad hypothesis, so it rejects back to 假设 (index 2), not all the way to
///   证据.
/// - **Build** (规格/任务分解/实现/评审合入·CI门禁): 规格 generates the intent
///   artifact, 任务分解 is pure planning (`Neutral`), 实现 generates the code
///   (`Generator`), 评审合入·CI门禁 is an explicit self-review gate
///   (`Evaluator`) that rejects back to 实现 (index 2) to fix.
/// - **Optimize** (基线测量/瓶颈定位/优化删减/回归验证): measurement and
///   diagnosis are `Neutral`; 优化/删减 is literally the optimizer role
///   (`Optimizer`); 回归验证 is the regression gate (`Evaluator`), rejecting
///   back to 优化/删减 (index 2).
/// - **Growth** (漏斗诊断/实验设计/A-B上线/放大或废弃): diagnosis is
///   `Neutral`; 实验设计 and 上线改动 each produce something new
///   (`Generator`); 放大或废弃 is the verdict gate (`Evaluator`) — "废弃"
///   means the experiment design was wrong, so it rejects back to 实验设计
///   (index 1).
/// - **Ops** (SLO/监控告警/事故响应/复盘回灌): none of these four is a
///   generate-then-judge pair within this one workflow — 复盘回灌 flows
///   forward into the *next* cycle's Prototype stage (the macro stage-loop
///   CLAUDE.md calls "运维复盘回流原型"), which is cross-workflow and out of
///   this spec's own phase indices, not a same-workflow reject target. All
///   four stay `Neutral`, honestly — no evaluator gate is machine-stamped in
///   just to fill the variant.
///
/// T16 (plan/12 §10 v1.1#3) adds each phase's real `agent`/`skills` binding.
/// Judged the same "read what actually happens" way as `role`/
/// `reject_to_phase` above, not stamped mechanically — and what actually
/// happens is uniform *within* a stage: [`rendered_phase_prompts`] injects
/// the identical [`role_preamble`] + [`skills_block`] into **every** phase of
/// the stage, including its Evaluator gate. A Build-stage "评审合入 · CI 门禁"
/// phase is not handed off to a separate reviewer BW doesn't have — its own
/// instruction says "以评审者身份**自查**": the *same* 构建师 wearing a
/// reviewer hat for one phase, still 构建师. So the honest binding is: every
/// phase's `agent` is this stage's real role agent
/// ([`role_agents`]'s `RoleAgent.name`, e.g. 构建师), and every phase's
/// `skills` are this stage's real working-method skills ([`stage_skills`]'s
/// names) — not a guess, a direct readback of what the rendered prompt
/// already injects into that exact phase.
pub fn phase_metas(kind: StageKind) -> Vec<crate::model::PhaseMeta> {
    use crate::model::{PhaseMeta, PhaseRole};
    const NEUTRAL: (PhaseRole, Option<u8>) = (PhaseRole::Neutral, None);
    let specs: &[(PhaseRole, Option<u8>)] = match kind {
        StageKind::Prototype => &[
            NEUTRAL,                         // 证据
            NEUTRAL,                         // 洞察
            (PhaseRole::Generator, None),    // 假设
            (PhaseRole::Generator, None),    // 原型
            (PhaseRole::Evaluator, Some(2)), // 验证 → 打回「假设」
        ],
        StageKind::Build => &[
            (PhaseRole::Generator, None),    // 规格 Spec
            NEUTRAL,                         // 任务分解
            (PhaseRole::Generator, None),    // Agent 并行实现
            (PhaseRole::Evaluator, Some(2)), // 评审合入 · CI 门禁 → 打回「实现」
        ],
        StageKind::Optimize => &[
            NEUTRAL,                         // 基线测量
            NEUTRAL,                         // 瓶颈定位
            (PhaseRole::Optimizer, None),    // 优化 / 删减
            (PhaseRole::Evaluator, Some(2)), // 回归验证 → 打回「优化/删减」
        ],
        StageKind::Growth => &[
            NEUTRAL,                         // 漏斗诊断
            (PhaseRole::Generator, None),    // 实验设计
            (PhaseRole::Generator, None),    // A/B 上线
            (PhaseRole::Evaluator, Some(1)), // 放大或废弃 → 打回「实验设计」
        ],
        StageKind::Ops => &[
            NEUTRAL, // SLO / 错误预算
            NEUTRAL, // 监控告警
            NEUTRAL, // 事故响应
            NEUTRAL, // 复盘回灌(流向下一圈「原型」,跨 workflow,非本流程内打回目标)
        ],
    };
    let agent_name = kind.role_short().to_string();
    let skill_names: Vec<String> = stage_skills(kind)
        .iter()
        .map(|s| s.name.to_string())
        .collect();

    kind.method_loop()
        .iter()
        .zip(specs.iter())
        .map(|(name, (role, reject_to_phase))| PhaseMeta {
            name: name.to_string(),
            role: *role,
            reject_to_phase: *reject_to_phase,
            agent: Some(agent_name.clone()),
            skills: skill_names.clone(),
        })
        .collect()
}

/// [`phase_metas`] for a **Dynamic** spec (`stage_workflow`/
/// `stage_workflow_with_playbook`): identical names/roles/agent/skills, but
/// every reject target is cleared to `None` — plan/12 §4's rule that a
/// Dynamic workflow never fixes the reject target at design time; it's the
/// (not-yet-built, T9) runtime evaluator's real verdict to make. The T16
/// `agent`/`skills` binding is untouched here — Static-vs-Dynamic is only a
/// reject-target policy difference, not a "who's actually running this"
/// difference (the same single agent runs either way).
pub fn phase_metas_dynamic(kind: StageKind) -> Vec<crate::model::PhaseMeta> {
    phase_metas(kind)
        .into_iter()
        .map(|mut p| {
            p.reject_to_phase = None;
            p
        })
        .collect()
}

/// One stage's working-method skill: a real, compact markdown instruction
/// block a real executor follows — the *executable* counterpart of the Skill
/// Hub's catalog cards. Same nature as [`phase_instructions`]: methodology in
/// code, generic across projects, never per-project content.
#[derive(Clone, Copy, Debug)]
pub struct StageSkill {
    /// Stable kebab-case name — the join key between the spec's `SkillRef`,
    /// the seeded Skill-Hub row, and run-time usage accounting.
    pub name: &'static str,
    /// One-line description (the hub card's `desc`).
    pub def: &'static str,
    /// The skill body — real instructions, injected verbatim into every
    /// phase prompt of the stage that carries it.
    pub content: &'static str,
}

/// The working-method skills each stage's role brings to a run. Small on
/// purpose: one to two per stage, each dense enough to change behavior, short
/// enough to inject into every phase without drowning the task itself.
pub fn stage_skills(kind: StageKind) -> &'static [StageSkill] {
    match kind {
        StageKind::Prototype => &[StageSkill {
            name: "evidence-first",
            def: "证据先行:只写站得住的内容,标注未核实",
            content: "### 证据先行 (evidence-first)\n\
                 1. 只记录两类内容:(a) 你直接验证过的事实(真实命令输出、真实文件内容);\
                 (b) 你的先验知识——必须标注「未核实」。\n\
                 2. 每条证据注明来源:文件路径、命令、或「知识截止内记忆,未核实」。\n\
                 3. 禁止编造统计数字与引用;没有可靠数字就写「无可靠数字」。\n\
                 4. 结论按「证据 → 洞察 → 假设」链书写,断链处如实标断。",
        }],
        StageKind::Build => &[StageSkill {
            name: "spec-to-tests",
            def: "规格即测试:每条验收标准落成一个可跑的用例",
            content: "### 规格即测试 (spec-to-tests)\n\
                 1. SPEC 里每条验收标准编号(AC-1, AC-2, …);写实现前先把它翻译成测试名\
                 (如 `ac1_reports_dead_relative_link`)。\n\
                 2. 无法翻译成测试的验收标准是坏标准——回头改写它,而不是跳过。\n\
                 3. 实现只做到让测试通过为止,不做规格外功能。\n\
                 4. 提交前 `cargo test` 全绿是硬门禁;失败输出原样留档,不美化。",
        }],
        StageKind::Optimize => &[StageSkill {
            name: "baseline-before-touch",
            def: "先测基线再动手:无基线不优化,删减优先",
            content: "### 先测基线再动手 (baseline-before-touch)\n\
                 1. 动手前先真实测量并落盘:测试数、clippy 警告数、代码行数、构建耗时——\
                 全部来自真实命令输出的原样摘录。\n\
                 2. 每步重构保持测试全绿;一步只做一类等价变换。\n\
                 3. 删减优先:能删的代码是最好的优化,删除行数计入成果。\n\
                 4. 结束时用与基线完全相同的命令重测,报 delta;无 delta 也如实报。",
        }],
        StageKind::Growth => &[StageSkill {
            name: "fresh-eyes-funnel",
            def: "新用户漏斗走查:亲手走一遍,只记录真实摩擦",
            content: "### 新用户漏斗走查 (fresh-eyes-funnel)\n\
                 1. 以从未见过本项目的人的视角,真实执行「发现 → 安装 → 首次使用 → 再次使用」\
                 每一步,不跳步、不脑补。\n\
                 2. 只记录你真实遇到的摩擦(命令报错、文档缺失、参数不明),不臆想用户。\n\
                 3. 一次实验只改一个变量,改动前后用同一条真实命令对照。\n\
                 4. 没有真实流量就如实做「前后对照」,不假装有 A/B 分流。",
        }],
        StageKind::Ops => &[StageSkill {
            name: "breaking-drill",
            def: "破坏性演练:拿坏输入砸,坏行为当场修",
            content: "### 破坏性演练 (breaking-drill)\n\
                 1. 系统性地喂坏输入:不存在的路径、空输入、超长输入、坏参数、坏编码——\
                 逐个真实执行并原样记录行为。\n\
                 2. 任何 panic 或不知所云的报错都算事故:当场修复成友好报错,修后重测。\n\
                 3. 健康检查脚本必须一键可跑、任何失败以非零码退出;写完真实执行一遍留档。\n\
                 4. 复盘只引用真实存在的文件与提交号(写之前 ls / git log 核实)。",
        }],
    }
}

/// The role-as-agent identity for one stage — what the Agent Hub's five
/// standing role agents are seeded from, and what a run's `AgentRef` points
/// back to. `instructions` is the *template* (with `{var}` slots): that is
/// honestly what the role gets told, with project specifics filled in at run
/// time by [`render`].
pub struct RoleAgent {
    /// `role_short()` — the join key (spec `AgentRef.name` ↔ agent row name).
    pub name: &'static str,
    pub role: String,
    pub skills: Vec<String>,
    /// Honest model label: execution rides the configured `claude` CLI; the
    /// workbench does not pin a model per role.
    pub model: &'static str,
    pub instructions: String,
}

/// Build the five standing role agents. Pure projection of stage metadata +
/// [`role_preamble`] — nothing here is invented per call.
pub fn role_agents() -> Vec<(StageKind, RoleAgent)> {
    StageKind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind,
                RoleAgent {
                    name: kind.role_short(),
                    role: format!("{} · {}段执行者", kind.methodology(), kind.label()),
                    skills: stage_skills(kind)
                        .iter()
                        .map(|s| s.name.to_string())
                        .collect(),
                    model: "claude CLI · 跟随执行器配置",
                    instructions: role_preamble(kind),
                },
            )
        })
        .collect()
}

/// The "## 技能(工作方法)" block appended to each phase prompt — the stage's
/// skills made *operative* (real content in the real prompt), not a name-only
/// advisory hint. Empty string when the stage has no skills.
fn skills_block(kind: StageKind) -> String {
    let skills = stage_skills(kind);
    if skills.is_empty() {
        return String::new();
    }
    let body = skills
        .iter()
        .map(|s| s.content)
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("\n## 技能(工作方法,本阶段全程适用)\n{body}\n")
}

/// Render the full per-phase prompt list for one stage: role preamble +
/// project context + the stage's skill blocks + the phase's own instruction.
/// Index-aligned with [`StageKind::method_loop`].
pub fn rendered_phase_prompts(kind: StageKind, ctx: &PlaybookCtx) -> Vec<String> {
    let preamble = render(&role_preamble(kind), ctx);
    let skills = skills_block(kind);
    phase_instructions(kind)
        .iter()
        .map(|instr| format!("{preamble}{skills}\n## 当前任务\n{}", render(instr, ctx)))
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
