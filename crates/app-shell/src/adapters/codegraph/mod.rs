//! `codegraph` 子进程封装。**装了就现跑,没装就如实灰**——不缓存、不入库、
//! 不入仓,每次打开页签就是一次全新的子进程调用。
//!
//! 只做「摆出原始数字」这一件事。**不做死代码判定**:BW 大量用 `dyn Trait`
//! 动态派发,`callers` 会漏边(预研实测),「零调用者」不能当死代码结论。

use std::path::Path;
use std::process::Command;

/// 三态。灰的两种各有各的下一步,不混成一句「不可用」。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    /// 命令不在 PATH 里。
    NotInstalled,
    /// 装了,但这个仓没跑过 `codegraph init`(没有 `.codegraph/`)。
    NotIndexed,
    Ready,
}

impl Availability {
    /// 灰态文案。装法里的版本号跟着 `scripts/codegraph-version` 走。
    pub fn hint(&self) -> String {
        match self {
            Availability::NotInstalled => format!(
                "本机没装 codegraph。装:npm install --global @colbymchenry/codegraph@{}\n\
                 装完在项目仓里跑一次 codegraph init。",
                VERSION
            ),
            Availability::NotIndexed => {
                "装了 codegraph,但这个仓还没建索引。在项目仓里跑一次 codegraph init。".into()
            }
            Availability::Ready => String::new(),
        }
    }
}

/// CI 钉住的版本(`scripts/codegraph-version`)。文案里给的装法要跟它一致,
/// 不然人装出来的版本和仓里 CI 跑的不是一个。
pub const VERSION: &str = include_str!("../../../../../scripts/codegraph-version");

pub fn detect(workspace: &Path) -> Availability {
    if which().is_none() {
        return Availability::NotInstalled;
    }
    if !workspace.join(".codegraph").is_dir() {
        return Availability::NotIndexed;
    }
    Availability::Ready
}

fn which() -> Option<String> {
    let out = Command::new("sh")
        .arg("-c")
        .arg("command -v codegraph")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!p.is_empty()).then_some(p)
}

/// 大文件榜的一行。字段就是 `codegraph files -j` 吐出来的原始字段,不加工。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileRow {
    pub path: String,
    pub language: String,
    pub node_count: u64,
    pub size: u64,
}

/// 现跑 `codegraph files -j`,按体积排序取前 `top` 行。
///
/// 非 0 退出把 stderr 原文带回去,**不吞错误**;JSON 解析不了也如实说,不
/// 悄悄返回一个空榜(空榜和「跑失败了」在界面上是两件事)。
pub fn big_files(workspace: &Path, top: usize) -> Result<Vec<FileRow>, String> {
    let out = Command::new("codegraph")
        .args(["files", "-j"])
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("起 codegraph 子进程失败:{e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("codegraph files 退出码 {:?}", out.status.code())
        } else {
            err
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("codegraph 的输出不是 JSON:{e}"))?;
    // 顶层可能是数组,也可能包一层 `{"files": [...]}`。两种都认,认不出就如实报错。
    let arr = v
        .as_array()
        .or_else(|| v.get("files").and_then(|f| f.as_array()))
        .ok_or_else(|| "codegraph 的输出既不是数组也没有 files 字段".to_string())?;
    let mut rows: Vec<FileRow> = arr
        .iter()
        .map(|e| FileRow {
            path: e
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or_default()
                .to_string(),
            language: e
                .get("language")
                .and_then(|p| p.as_str())
                .unwrap_or_default()
                .to_string(),
            node_count: e.get("nodeCount").and_then(|n| n.as_u64()).unwrap_or(0),
            size: e.get("size").and_then(|n| n.as_u64()).unwrap_or(0),
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.size));
    rows.truncate(top);
    Ok(rows)
}
