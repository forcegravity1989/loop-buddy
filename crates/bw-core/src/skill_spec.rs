//! plan/16 · Skill 标准规范(v1)的机检 — the one shared checker behind all
//! four防线 (命令层守卫 / Boot 自愈对账的 pristine 判定 / SkillHub 徽记 /
//! `audit_skills` 指挥器). Pure string checks, zero IO, wasm32-clean — the
//! same "one source of truth, no second copy" rule `ui::skill_file_tree`
//! already follows.
//!
//! Rule numbers (S1/S3/S4/S5, A1/A2/A3) are plan/16 §1's own table — the
//! spec doc and this module must move together. S2 (name uniqueness) and S6
//! (raw source-tag consistency) are deliberately absent here: both need
//! whole-library / raw-storage context a per-skill pure function honestly
//! doesn't have — they live in `audit_skills` (and S6 additionally in Boot's
//! pristine promotion).
//!
//! 分域执行 (plan/16 §1): findings against an **official-library import**
//! are all [`SpecSeverity::Advisory`] — the body/metadata is another
//! library's verbatim text, honestly reported but never rewritten in place
//! (校正 = upstream re-import, or a local edit which T11 flips to
//! `SelfBuilt`, after which the row is BW-owned and hard rules apply).
//! BW-authored rows get [`SpecSeverity::MustFix`] for the S-rules; A-rules
//! stay Advisory for everyone.

/// How a finding should be acted on — the badge/audit tier, not a guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecSeverity {
    /// 硬规违规:必须校正(黄徽记「待校正」计数的口径)。
    MustFix,
    /// 提示:如实报告,不自动改写(官方外库原文、超长正文等)。
    Advisory,
}

/// One rule violation, ready to render verbatim (详情面板 / 审计报告行).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecFinding {
    /// plan/16 §1 rule id, e.g. `"S1"` — stable join key to the spec doc.
    pub rule: &'static str,
    pub severity: SpecSeverity,
    /// 人话违规说明(中文,与 UI/审计共用同一句,不各写一份)。
    pub message: String,
}

/// The per-skill fields the spec rules read. Borrowed, because every caller
/// (VM projection, command guard, audit loop) already owns the strings.
#[derive(Clone, Copy, Debug)]
pub struct SkillSpecInput<'a> {
    pub name: &'a str,
    pub desc: &'a str,
    pub content: &'a str,
    /// `true` = 官方外库导入原文(`HubSource::Official` 且非 bw-standard):
    /// 一切 finding 降为 Advisory。BW 自带标准库(bw-standard)是自产,受硬规。
    pub official_import: bool,
    /// `distilled_from_issue.is_some()` — A3 的输入之一。
    pub distilled: bool,
    /// `origin_agent.is_some()` — A3 的另一半。
    pub has_origin_agent: bool,
}

/// S1: `^[a-z0-9]+(-[a-z0-9]+)*$`, 1-64 chars — agentskills.io's exact name
/// grammar (lowercase alphanumerics + single interior hyphens), hand-rolled
/// so the kernel takes no regex dependency.
pub fn is_valid_skill_name(name: &str) -> bool {
    let n = name.chars().count();
    if n == 0 || n > 64 {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// S4's mechanical trigger-segment check (plan/16 §1 的机检口径,原样):
/// 中文「适用」,或英文 "use when" / "use this" / "use it"(大小写不敏感)。
/// 一个说清「何时用」的 description 未必非用这些词不可——但 BW 自产技能
/// 采用统一 house style,机器才可检,规范才不靠自觉。
pub fn desc_has_trigger(desc: &str) -> bool {
    if desc.contains("适用") {
        return true;
    }
    let lower = desc.to_lowercase();
    lower.contains("use when") || lower.contains("use this") || lower.contains("use it")
}

/// Run every per-skill rule; empty result = 合规(徽记不出声,绿色隐身).
pub fn check_skill(input: SkillSpecInput<'_>) -> Vec<SpecFinding> {
    let hard = |sev_pool: &mut Vec<SpecFinding>, rule, message: String| {
        sev_pool.push(SpecFinding {
            rule,
            severity: if input.official_import {
                SpecSeverity::Advisory
            } else {
                SpecSeverity::MustFix
            },
            message,
        });
    };
    let advisory = |sev_pool: &mut Vec<SpecFinding>, rule, message: String| {
        sev_pool.push(SpecFinding {
            rule,
            severity: SpecSeverity::Advisory,
            message,
        });
    };

    let mut out = Vec::new();

    // ── S1 · name 格式 ──
    if !is_valid_skill_name(input.name) {
        hard(
            &mut out,
            "S1",
            "名称须为 1-64 字符的小写 kebab-case(字母/数字/单连字符,如 evidence-first)".to_string(),
        );
    }

    // ── S3 · desc 非空且 ≤1024 字符 ──
    let desc_chars = input.desc.chars().count();
    if input.desc.trim().is_empty() {
        hard(
            &mut out,
            "S3",
            "描述为空——描述是 agent 决定何时加载技能的唯一依据".to_string(),
        );
    } else if desc_chars > 1024 {
        hard(
            &mut out,
            "S3",
            format!("描述 {desc_chars} 字符,超出规范上限 1024"),
        );
    }

    // ── S4 · desc 含「何时用」触发段(desc 为空时 S3 已报,不叠报) ──
    if !input.desc.trim().is_empty() && !desc_has_trigger(input.desc) {
        hard(
            &mut out,
            "S4",
            "描述缺「何时用」触发段(中文「适用:…」/英文 \"Use when …\")".to_string(),
        );
    }

    // ── S5 · 正文非空 ──
    if input.content.trim().is_empty() {
        hard(
            &mut out,
            "S5",
            "正文为空——技能是可执行的方法,不是收藏夹书签".to_string(),
        );
    }

    // ── A1 · 正文 ≤500 行 ──
    let lines = input.content.lines().count();
    if lines > 500 {
        advisory(
            &mut out,
            "A1",
            format!("正文 {lines} 行,超出最佳实践建议的 500 行——细节宜拆支撑文件渐进披露"),
        );
    }

    // ── A2 · 保留字 ──
    if input.name.contains("claude") || input.name.contains("anthropic") {
        advisory(
            &mut out,
            "A2",
            "名称含保留字 claude/anthropic(Anthropic 最佳实践不建议)".to_string(),
        );
    }

    // ── A3 · 蒸馏溯源成对 ──
    if input.distilled && !input.has_origin_agent {
        advisory(
            &mut out,
            "A3",
            "蒸馏溯源缺损:有 distilled_from_issue 却无 origin_agent——系统派生字段,须排查,不许手补"
                .to_string(),
        );
    }

    out
}

/// 徽记口径:待校正数 = MustFix 条数(Advisory 不点黄灯,详情里如实列出)。
pub fn must_fix_count(findings: &[SpecFinding]) -> usize {
    findings
        .iter()
        .filter(|f| f.severity == SpecSeverity::MustFix)
        .count()
}
