//! 技能物化:把本阶段的候选技能写成工作区里真实的 `.claude/skills/<name>/`
//! 包,由 `claude` CLI 用它原生的 skill 加载机制按需读。
//!
//! **为什么不直接把正文塞进 prompt**:注入护栏 `skills_prompt_block` 的硬上限
//! 是 6000 字符,而 superpowers 技能正文平均 8732 字符(实测,2026-08-05)——
//! 单独一条就撑爆整个预算。按阶段挑正文塞 prompt 的做法,对绝大多数外部技能
//! 是空转,归类的「牙齿」是假的。
//!
//! **绝不覆盖用户手写**:同名目录若没有 `.bw-managed` 标记,整条跳过并留痕。
//! 用户自己在工作区写的 skill 永远优先于 BW 托管的派生副本。

use bw_core::model::SkillCard;
use bw_store::SkillFileRow;
use std::path::Path;

/// BW 托管标记文件名。内容是 `<skill id>\n<rev>`,既标身份也标版本 —— 版本
/// 一致就整条跳过,不做无谓写盘。
const MANAGED_MARKER: &str = ".bw-managed";

/// 一次物化的真实结果。字段都是**发生过的事**,不是计划 —— 调用方据此留痕。
#[derive(Debug, Default)]
pub(crate) struct MaterializeReport {
    /// 真写了盘的技能名。
    pub written: Vec<String>,
    /// 版本一致、整条跳过的技能数。
    pub unchanged: usize,
    /// 同名目录存在但**不是** BW 托管(没有标记文件)因而跳过的技能名 ——
    /// 这些是用户自己的 skill,优先级高于我们。
    pub skipped_foreign: Vec<String>,
}

/// 把 `skills`(每项 = 技能行 + 它的支撑文件)物化到 `workspace/.claude/skills/`。
///
/// `rev` 用于版本比对:调用方传入的 `SkillCard` 没有 `rev` 字段,所以这里用
/// 「id + 正文长度 + 支撑文件数」拼一个稳定指纹 —— 它对本用途足够:同一件技能
/// 内容没变就不重写,变了必然重写。
pub(crate) async fn materialize_stage_skills(
    workspace: &str,
    skills: &[(SkillCard, Vec<SkillFileRow>)],
) -> MaterializeReport {
    let mut report = MaterializeReport::default();
    let ws = workspace.trim();
    if ws.is_empty() {
        return report; // 未配置真实工作区 —— no-op,零报错
    }
    let root = Path::new(ws);
    for (skill, files) in skills {
        let dir_rel = format!(".claude/skills/{}", skill.name);
        let marker_rel = format!("{dir_rel}/{MANAGED_MARKER}");
        let fingerprint = format!("{}\n{}", skill.id.uuid(), skill.content.len() + files.len());
        let dir_abs = root.join(&dir_rel);
        let marker_abs = root.join(&marker_rel);

        if dir_abs.exists() {
            match tokio::fs::read_to_string(&marker_abs).await {
                Ok(existing) if existing.trim() == fingerprint.trim() => {
                    report.unchanged += 1;
                    continue;
                }
                Ok(_) => { /* BW 托管但版本不同 —— 往下重写 */ }
                Err(_) => {
                    // 目录在、标记不在 = 用户自己的 skill。绝不动。
                    report.skipped_foreign.push(skill.name.clone());
                    continue;
                }
            }
        }

        // 正文**原样**写出,不做 demote_headings —— 那是嵌套进 prompt 块才需要
        // 的变换;独立的 SKILL.md 必须保持 `#` 开头的原形,否则 CLI 认不出。
        let mut ok =
            bw_engine::workspace::write_file(root, &format!("{dir_rel}/SKILL.md"), &skill.content)
                .await
                .is_ok();
        for f in files {
            if bw_engine::workspace::write_file(
                root,
                &format!("{dir_rel}/{}", f.rel_path),
                &f.content,
            )
            .await
            .is_err()
            {
                ok = false;
            }
        }
        if ok
            && bw_engine::workspace::write_file(root, &marker_rel, &fingerprint)
                .await
                .is_ok()
        {
            report.written.push(skill.name.clone());
        }
        // 写盘失败不炸 run —— 物化是增益,不是运行前提。失败的那条不进
        // `written`,报告因此如实。
    }
    report
}
