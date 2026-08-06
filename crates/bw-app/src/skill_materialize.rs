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
    /// 评审找出的真坑(2026-08-06):技能名 / 支撑文件 `rel_path` 都来自未经
    /// `guard_skill_name` 校验的来源(`ImportSkillPackage`/`ImportSkillLibrary`
    /// 刻意放行外部库文本逐字进入,见 `lib.rs` 的 `guard_skill_name` 文档
    /// 注释)——本物化器是第一个把这些未经校验的字符串拼成真实写盘路径的
    /// 消费者,必须自己做纵深防御。含 `..`/绝对路径/空段、或规范化后仍落在
    /// `.claude/skills/<name>/` 之外的技能,整条跳过,记在这里、不当普通
    /// `skipped_foreign` 处理(那是"用户的东西",这是"路径穿越拦截")。
    pub skipped_unsafe: Vec<String>,
}

/// 纵深防御(finding #3):`base` 是允许落盘的根(`.claude/skills` 或某个
/// 技能自己的目录),`rel` 是待拼接的相对片段(技能名本身,或一个
/// `SkillFileRow.rel_path`)。规则:非空、不含 `..`、不以 `/` 开头、无空段
/// (含开头/结尾/连续的 `/`);再做一次纯词法规范化(不摸文件系统 —— 目标
/// 文件多数还不存在,`canonicalize` 用不了)——只允许 `Normal` 分量拼接,
/// 最终路径必须仍然落在 `base` 之内。任何一条不过就是不安全。
fn is_safe_rel_path(base: &Path, rel: &str) -> bool {
    if rel.is_empty() || rel.contains("..") || rel.starts_with('/') {
        return false;
    }
    if rel.split('/').any(|seg| seg.is_empty()) {
        return false;
    }
    let mut joined = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            std::path::Component::Normal(seg) => joined.push(seg),
            // ParentDir/RootDir/CurDir/Prefix —— 一律拒,纵深防御不信任
            // 上面字符串级检查已经查干净,双保险。
            _ => return false,
        }
    }
    joined.starts_with(base)
}

/// 把 `skills`(每项 = 技能行 + 它的支撑文件)物化到 `workspace/.claude/skills/`。
///
/// 指纹 = `skill id + rev`(spec §5.3):`rev` 是 `skill.rev` 列真实的单调
/// 版本号,每次内容编辑 `update_skill` 都 `rev=rev+1` —— 任何一次真实编辑
/// (哪怕字节数不变,比如改个错别字)都必然让指纹改变,不会像旧版「id + 正文
/// 长度 + 支撑文件数」那样在同长度编辑下误判「未变」而把过期的 SKILL.md
/// 留在磁盘上。
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
    let skills_root = root.join(".claude/skills");
    for (skill, files) in skills {
        if !is_safe_rel_path(&skills_root, &skill.name) {
            eprintln!(
                "[BW_SKILL_MATERIALIZE_SECURITY] 拦截技能名穿越:{:?} 校验不过(含 .. / 绝对路径 / 空段,或规范化后逃出 .claude/skills/),整条跳过、未写盘",
                skill.name
            );
            report.skipped_unsafe.push(skill.name.clone());
            continue;
        }
        let dir_rel = format!(".claude/skills/{}", skill.name);
        let dir_abs = root.join(&dir_rel);
        if let Some(bad) = files
            .iter()
            .find(|f| !is_safe_rel_path(&dir_abs, &f.rel_path))
        {
            eprintln!(
                "[BW_SKILL_MATERIALIZE_SECURITY] 拦截技能 {:?} 的支撑文件路径穿越:rel_path={:?} 校验不过,整条技能跳过、未写盘",
                skill.name, bad.rel_path
            );
            report.skipped_unsafe.push(skill.name.clone());
            continue;
        }
        let marker_rel = format!("{dir_rel}/{MANAGED_MARKER}");
        let fingerprint = format!("{}\n{}", skill.id.uuid(), skill.rev);
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
