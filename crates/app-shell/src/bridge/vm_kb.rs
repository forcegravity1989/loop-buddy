//! 知识库屏三个页签的 ViewModel 拼装:知识(仓内文档树)、代码图、资产。
//!
//! 三个页签的共同点:**一张登记表都不查**。库里只有四张表,这里的每个数字
//! 都是打开那一刻现扫仓目录、现走 git、现起子进程得来的。

use crate::vm::*;
use bw_v4::model::ProjectId;
use bw_v4::repo::{managed_file, release_file};
use bw_v4::V4Store;
use std::path::Path;

/// 铺底管账里登记过的文件路径。**一次读完传下去** —— 原来是每个技能包各读
/// 一次、各解析一次 `.bw/managed.toml`,十几个包就是十几次全文解析,而这段
/// 每重拼一次 ViewModel 就跑一遍。
pub(super) fn managed_paths(ws: &Path) -> Vec<String> {
    managed_file::read(ws)
        .ok()
        .flatten()
        .map(|m| m.files.into_iter().map(|e| e.path).collect())
        .unwrap_or_default()
}

/// 只读文件开头这么多字节。周文件的徽记就在 front matter 里,为了一行标记把
/// 一份几十 KB 的周计划整篇读进内存不值 —— 老项目回填后这里有上百份。
const HEAD_BYTES: usize = 512;

fn read_head(path: &Path) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = vec![0u8; HEAD_BYTES];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    // 截断可能切在半个汉字上,按有损转换 —— 这段只用来找一行 ASCII 标记。
    String::from_utf8_lossy(&buf).into_owned()
}

/// 知识页签的树:**不扫全仓**,按规范八大类固定分组、每组按约定路径找文件。
/// 只列真实存在的,不列位置。
pub(super) fn build_kb(ws: &Path, tab: KbTab, open: Option<&str>) -> KbVm {
    let managed_count = managed_paths(ws).len();
    let mut groups = Vec::new();

    let mut charter = Vec::new();
    for (rel, label) in [("PROJECT.md", "项目章程"), ("AGENTS.md", "给 agent 的规矩")] {
        if ws.join(rel).is_file() {
            charter.push(KbFileVm {
                rel: rel.into(),
                label: format!("{rel} · {label}"),
                badge: String::new(),
            });
        }
    }
    push_group(&mut groups, "章程", charter);

    let mut spec = Vec::new();
    for (rel, label) in [
        (".bw/project.toml", "项目名片"),
        (".bw/metrics.toml", "指标"),
        (".bw/issue-policy.toml", "类别映射与节律"),
        (".bw/standard.toml", "铺了哪版规范"),
        (".bw/managed.toml", "铺底文件的指纹"),
    ] {
        if ws.join(rel).is_file() {
            spec.push(KbFileVm {
                rel: rel.into(),
                label: format!("{rel} · {label}"),
                badge: String::new(),
            });
        }
    }
    push_group(&mut groups, "规范件", spec);

    // 周计划倒序:最近的在最上面。回填出来的和人写的同目录同格式,只挂一个徽记。
    let mut weeks: Vec<KbFileVm> = std::fs::read_dir(ws.join("docs/plan"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let rel = format!("docs/plan/{name}");
                    // 徽记只看头里那几行,不整篇读 —— 老项目回填后这里可能上百份,
                    // 而这段每重拼一次 ViewModel 就跑一遍。
                    let backfilled = read_head(&e.path())
                        .lines()
                        .take(12)
                        .any(|l| l.trim() == "origin: backfill");
                    KbFileVm {
                        label: name.trim_end_matches(".md").to_string(),
                        rel,
                        badge: if backfilled {
                            "回填".into()
                        } else {
                            String::new()
                        },
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    weeks.sort_by(|a, b| b.label.cmp(&a.label));
    push_group(&mut groups, "周计划", weeks);

    let mut rel_files = Vec::new();
    if ws.join("docs/releases.md").is_file() {
        rel_files.push(KbFileVm {
            rel: "docs/releases.md".into(),
            label: "发版记录".into(),
            badge: String::new(),
        });
    }
    push_group(&mut groups, "发版记录", rel_files);

    for (dir, title) in [("docs/decisions", "决策记录"), ("docs/design", "设计产物")] {
        let mut files: Vec<KbFileVm> = std::fs::read_dir(ws.join(dir))
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        KbFileVm {
                            label: name.trim_end_matches(".md").to_string(),
                            rel: format!("{dir}/{name}"),
                            badge: String::new(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by(|a, b| a.label.cmp(&b.label));
        push_group(&mut groups, title, files);
    }

    KbVm {
        tab,
        open_doc: open.and_then(|rel| {
            std::fs::read_to_string(ws.join(rel))
                .map(|body| (rel.to_string(), body))
                // 读不动就把原话摆出来,不静默变成「没打开」。
                .or_else(|e| Ok::<_, std::io::Error>((rel.to_string(), format!("读取失败:{e}"))))
                .ok()
        }),
        groups,
        managed_count,
        codegraph: None,
        assets: None,
    }
}

fn push_group(out: &mut Vec<KbGroupVm>, title: &str, files: Vec<KbFileVm>) {
    // 某组一个文件都没有就整组不显示 —— 空标题比没有更让人以为出了错。
    if files.is_empty() {
        return;
    }
    out.push(KbGroupVm {
        title: title.to_string(),
        files,
    });
}

/// 代码图页签:每次点开就是一次全新的子进程调用,不缓存。
pub(super) async fn build_codegraph(ws: &Path) -> CodeGraphVm {
    use crate::adapters::codegraph::{big_files, detect, Availability};
    let state = detect(ws).await;
    if state != Availability::Ready {
        return CodeGraphVm {
            state: match state {
                Availability::NotInstalled => "not_installed".into(),
                _ => "not_indexed".into(),
            },
            hint: state.hint(),
            ..CodeGraphVm::default()
        };
    }
    match big_files(ws, 20).await {
        Ok(rows) => CodeGraphVm {
            state: "ready".into(),
            rows: rows
                .into_iter()
                .map(|r| CodeFileVm {
                    path: r.path,
                    language: r.language,
                    nodes: r.node_count,
                    size: r.size,
                })
                .collect(),
            ..CodeGraphVm::default()
        },
        Err(e) => CodeGraphVm {
            state: "ready".into(),
            error: e,
            ..CodeGraphVm::default()
        },
    }
}

/// 资产页签:五个区块**全部现算**——扫仓目录、走 git、解析仓文件。
/// 没有登记表可查(V4 库里只有四张表)。
pub(super) async fn build_assets(store: &V4Store, id: ProjectId, ws: &Path) -> AssetsVm {
    let usage = store.workflow_usage(id).await.unwrap_or_default();
    let skills = super::vm_panels::skill_list(ws, &usage);

    let artifacts = bw_v4::git::artifacts(ws, 200)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|a| ArtifactVm {
            path: a.path,
            commit: a.commit,
            subject: a.subject,
            issue: a.issue_number.map(|n| format!("#{n}")).unwrap_or_default(),
        })
        .collect();

    let releases = release_file::read(ws)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .rev()
        .map(|r| ReleaseVm {
            version: r.version,
            released_at: r.released_at,
            note: r.note,
            included: if r.included_numbers.is_empty() {
                String::new()
            } else {
                r.included_numbers
                    .iter()
                    .map(|n| format!("#{n}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            },
            origin: r.origin,
        })
        .collect();

    let (repo_stats, error) = match bw_engine::evidence::collect(&ws.display().to_string()).await {
        Ok(e) => (
            vec![
                ("提交数".to_string(), e.commit_count.to_string()),
                ("跟踪的文件".to_string(), e.tracked_files.to_string()),
                ("没提交的改动".to_string(), e.dirty_paths.to_string()),
                ("docs/ 下的 .md".to_string(), e.docs_files.to_string()),
            ],
            String::new(),
        ),
        Err(e) => (Vec::new(), format!("读不到仓统计:{e}")),
    };

    AssetsVm {
        skills,
        // V4 还没有蒸馏这颗按钮(见 docs/LEFTOVERS.md V4B-6),所以这里恒空。
        // 不放占位数据。
        distilled: Vec::new(),
        artifacts,
        releases,
        repo_stats,
        error,
    }
}
