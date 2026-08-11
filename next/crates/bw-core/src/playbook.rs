//! Role playbook context + template rendering.
//!
//! next 减法专项(2026-08):这个模块曾经是 v1 的完整可执行剧本系统——按
//! 阶段渲染 phase 级 prompt、绑 role agent、注入工作方法 skill(
//! `role_preamble`/`phase_instructions`/`phase_metas`/`StageSkill`/
//! `RoleAgent`/`rendered_phase_prompts`/`stage_prompt` 等)。那一整套喂的是
//! `WorkflowSpec`/Hub 目录/Skill·Agent Card 系统——`next` 整包移植进来后
//! 这条消费链在这里从未接上(零消费者,死代码审计坐实),随同
//! `bw-core::model` 的 WorkflowSpec 构建管线/Hub 资产族一并删除。
//!
//! [`PlaybookCtx`] 与 [`render`] 保留:`bw-engine`(`interactive_cli.rs` /
//! `agentcli/connector.rs`)真消费它们组装交互式会话的项目上下文与开场
//! 系统提示——这条路径与被删的旧剧本渲染系统无关。
//!
//! Rendering is a plain `{var}` substitution — no template engine, no IO.

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
