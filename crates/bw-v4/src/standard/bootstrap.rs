//! 规范铺底第 1 步:buddy 自己把核心件写进项目仓,不起 agent。
//!
//! 三条规矩:
//!
//! 1. **人手改过的不覆盖**。只在目标路径不存在、或存在且指纹与
//!    `.bw/managed.toml` 记的一致时才写;两者都不满足就跳过,并把跳过的路径
//!    如实报出来(调用方写进这张活的说明里,评审的人不用猜为什么少了一份)。
//! 2. **指纹清单最后写**。`.bw/managed.toml` 记的是刚落盘那一刻的真实内容,
//!    先写它就会记成上一轮的旧值。
//! 3. **落盘方式看仓是不是 buddy 自己建的**。是(空仓例外)就直接提交当前
//!    分支;不是就交给调用方走分支与 MR —— 这一层只负责写文件,不决定怎么合。

use crate::repo::managed_file::{self, ManagedEntry, ManagedFile};
use crate::repo::{write_file, RepoFileError};
use crate::standard;
use std::path::Path;

/// 一次铺底第 1 步的结果。全部是真实发生过的事实,没有一项是「打算做」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BootstrapReport {
    /// 真的写进去的路径。
    pub written: Vec<String>,
    /// 跳过的路径 + 为什么跳。
    pub skipped: Vec<(String, String)>,
}

/// 渲染模板要用的变量。
#[derive(Debug, Clone, Default)]
pub struct BootstrapVars {
    pub name: String,
    pub brief: String,
    pub benchmark: String,
    pub north_star: String,
    pub remote: String,
    pub owner: String,
    pub current_version: String,
    pub chat: String,
}

/// 写规范件 + 记指纹。幂等:重跑一遍,已经一致的件原样跳过。
///
/// `all = false`(第 0 站接入)只铺 [`standard::LayAt::Adopt`] 那几份;
/// `all = true`(人在配置屏点「规范铺底」)把该有的都补齐。
pub fn write_core_files(
    workspace: &Path,
    vars: &BootstrapVars,
    all: bool,
) -> Result<BootstrapReport, RepoFileError> {
    let version = standard::version();
    let mut managed = managed_file::read(workspace)?.unwrap_or_default();
    let mut report = BootstrapReport::default();

    let vars_list: Vec<(&str, &str)> = vec![
        ("name", &vars.name),
        ("brief", &vars.brief),
        ("benchmark", &vars.benchmark),
        ("north_star", &vars.north_star),
        ("remote", &vars.remote),
        ("owner", &vars.owner),
        ("current_version", &vars.current_version),
        ("chat", &vars.chat),
        ("version", version),
    ];

    for tmpl in standard::CORE_TEMPLATES {
        // 第 0 站只铺人马上要用的那几份;其余等真的走到那一站,或者人在配置屏
        // 点一次「规范铺底」补齐。
        if !all && tmpl.lay_at != standard::LayAt::Adopt {
            continue;
        }
        let body = standard::render(tmpl.body, &vars_list);
        lay_one(
            workspace,
            tmpl.target,
            &body,
            version,
            &mut managed,
            &mut report,
        )?;
    }

    // **技能剧本不再复制进用户的仓。**
    //
    // 原来复制 13 份 SKILL.md 进去,是为了 buddy 自己等会儿再从那儿读一遍
    // 正文塞进 `--append-system-prompt` —— 绕了一圈,而正本本来就编在 buddy
    // 二进制里。代价还很实在:项目只要把 `.claude/` 写进 .gitignore(buddy
    // 自己的仓就是),这 13 份就进不了版本控制,只活在某一台机器的检出里,
    // 人根本说不清「bw 的资产到底是哪些」。
    //
    // 现在 buddy 把自带的剧本摊在自己的目录里,开工时按需给 agent 指路(见
    // `app::bootstrap::agent_system_prompt`)。真想让**你自己**在这个仓里敲
    // `claude` 时也能 `/skill-name`,那是另一件事,该做成「想要就铺一份」的
    // 选项,不是默认往每个仓里塞。

    // 指纹清单最后写 —— 记的必须是刚落盘那一刻的内容。
    managed_file::write(workspace, &managed)?;
    report.written.push(managed_file::REL_PATH.to_string());
    Ok(report)
}

fn lay_one(
    workspace: &Path,
    target: &str,
    body: &str,
    version: &str,
    managed: &mut ManagedFile,
    report: &mut BootstrapReport,
) -> Result<(), RepoFileError> {
    let path = workspace.join(target);
    let on_disk = match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(RepoFileError::Io {
                path: target.into(),
                source: e,
            })
        }
    };

    if let Some(existing) = &on_disk {
        match managed.entry(target) {
            // buddy 从没管过这个文件,而它已经在仓里 —— 那是项目自己的东西,不碰。
            None => {
                report.skipped.push((
                    target.to_string(),
                    "已存在且非 buddy 管理,跳过(不覆盖人手写的内容)".into(),
                ));
                return Ok(());
            }
            Some(entry) if managed_file::fingerprint(existing) != entry.fingerprint => {
                report.skipped.push((
                    target.to_string(),
                    "buddy 铺过之后有人改过,跳过(升级请走对账,不静默覆盖)".into(),
                ));
                return Ok(());
            }
            Some(_) => {}
        }
    }

    write_file(workspace, target, body)?;
    managed.upsert(ManagedEntry {
        path: target.to_string(),
        version: version.to_string(),
        fingerprint: managed_file::fingerprint(body.as_bytes()),
    });
    report.written.push(target.to_string());
    Ok(())
}

/// 铺底探测:这张活要跑几步。全部现算,不问用户。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BootstrapProbe {
    /// 仓是 buddy 自己建的(根提交作者是 buddy)—— 直推,不走 MR。
    pub owned: bool,
    /// 仓根已经有 README / CLAUDE.md / AGENTS.md —— 这个仓自己已经有一套约定了。
    ///
    /// **这条不触发铺底的任何一步。** 铺底只铺 `.bw/`(buddy 自己的资产),
    /// 仓根一个字不写。要不要给这个项目写一份开发手册,是资产盘点(运作活②)
    /// 首次模式去问人的事 —— 那是在改人家的项目,得先问过。这条只是写进活的
    /// 正文当证据,顺带给将来那次盘点当输入。
    pub has_own_conventions: bool,
    /// 有历史(提交数 > 1、有标签、有 CHANGELOG)—— 触发资产盘点首次模式(历史回填)。
    pub has_history: bool,
    /// 写进这张活说明里的证据句子。评审的人不用猜这张活为什么跑了这几步。
    pub reasons: Vec<String>,
}

/// buddy 自己 scaffold 出来的空仓,根提交作者就是这个名字。
const OWNED_AUTHOR: &str = "Builders' Workbench";

pub async fn probe(workspace: &Path) -> BootstrapProbe {
    let mut p = BootstrapProbe::default();
    if !crate::git::is_repo(workspace).await {
        p.reasons
            .push("这个目录还不是 git 仓,只写核心件、不提交".into());
        return p;
    }

    let author = crate::git::root_commit_author(workspace)
        .await
        .unwrap_or_default();
    p.owned = author == OWNED_AUTHOR;

    let commits = crate::git::commit_count(workspace).await.unwrap_or(0);
    let tags = crate::git::tags(workspace).await.unwrap_or_default();
    let has_changelog = ["CHANGELOG.md", "RELEASES.md", "CHANGELOG"]
        .iter()
        .any(|f| workspace.join(f).is_file());

    p.has_own_conventions = ["README.md", "CLAUDE.md", "AGENTS.md"]
        .iter()
        .any(|f| workspace.join(f).is_file());
    // 1 条提交 = buddy 自己那次 scaffold,不算历史。
    p.has_history = commits > 1 || !tags.is_empty() || has_changelog;

    p.reasons.push(format!("仓有 {commits} 条提交"));
    p.reasons.push(format!("{} 个标签", tags.len()));
    if p.has_own_conventions {
        p.reasons
            .push("仓根已有 README / CLAUDE.md / AGENTS.md(这个仓自己有约定,铺底不碰)".into());
    }
    if has_changelog {
        p.reasons.push("仓根有 CHANGELOG / RELEASES".into());
    }
    p
}

/// 这张活的标题:固定前缀 + 命中项后缀。写死进标题,是「打算跑什么」的快照。
/// 铺底那张活的标题。**只跟规范版本走,不跟探测结果走** —— 它同时是建活的
/// 幂等键,跟着探测结果变的话重跑就会多建一张活(第一次铺底自己写了
/// `CLAUDE.md` 并提交,第二次探测结论就不一样了)。
pub fn issue_title() -> String {
    format!("规范铺底 v{}", standard::version())
}

/// 探测到需要跑哪几步 —— 这句话写进活的正文,不写进标题。
pub fn planned_steps(probe: &BootstrapProbe) -> String {
    let mut steps = vec!["写核心件(全部落在 `.bw/` 下,仓根一个字不写)"];
    if probe.has_history {
        steps.push("资产盘点首次模式:历史回填(这个仓已经有历史了)");
    }
    steps.join("、")
}
