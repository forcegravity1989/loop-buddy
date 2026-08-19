//! `.bw/managed.toml` —— 哪些文件是 buddy 铺下去的、什么版本、铺那一刻的内容
//! 指纹。规范对账的唯一权威记录。
//!
//! 为什么是一份清单而不是给每个被铺的文件配一个同名旁路文件:铺一次底就在
//! 仓里撒一地隐藏文件太脏。
//!
//! 对账判定:清单里没有这条、或有记录但磁盘没这个文件 → 缺;磁盘指纹与记录
//! 一致但记录的版本比 buddy 自带的旧 → 过期;磁盘指纹与记录不一致 → 人改过
//! (不看版本号);其余 → 一致。**人改过的一律不覆盖,只提示差异**。

use super::{parse_toml, read_to_string, write_file, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const REL_PATH: &str = ".bw/managed.toml";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedEntry {
    pub path: String,
    pub version: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedFile {
    #[serde(default, rename = "file")]
    pub files: Vec<ManagedEntry>,
}

impl ManagedFile {
    pub fn entry(&self, path: &str) -> Option<&ManagedEntry> {
        self.files.iter().find(|f| f.path == path)
    }

    /// 铺底/升级时追加或更新一条。
    pub fn upsert(&mut self, entry: ManagedEntry) {
        match self.files.iter_mut().find(|f| f.path == entry.path) {
            Some(slot) => *slot = entry,
            None => self.files.push(entry),
        }
    }
}

/// 对账的四种结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconcile {
    /// 该有的件不在(没记录,或记录了但磁盘上没有)。
    Missing,
    /// 内容还是 buddy 铺的原样,但铺的时候用的是旧版规范。
    Stale,
    /// 人改过。不覆盖,只提示差异。
    HumanEdited,
    UpToDate,
}

/// 一个文件当前算什么状态。`disk` 为 `None` 表示磁盘上没有这个文件。
pub fn reconcile(
    entry: Option<&ManagedEntry>,
    disk: Option<&[u8]>,
    current_version: &str,
) -> Reconcile {
    let (Some(entry), Some(disk)) = (entry, disk) else {
        return Reconcile::Missing;
    };
    if fingerprint(disk) != entry.fingerprint {
        return Reconcile::HumanEdited;
    }
    if entry.version != current_version {
        return Reconcile::Stale;
    }
    Reconcile::UpToDate
}

/// `sha256:<64 位 hex>`。只在铺底/升级那一刻算一次。
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub fn parse(raw: &str) -> Result<ManagedFile> {
    parse_toml(REL_PATH, raw)
}

pub fn read(workspace: &Path) -> Result<Option<ManagedFile>> {
    match read_to_string(workspace, REL_PATH)? {
        None => Ok(None),
        Some(raw) => parse(&raw).map(Some),
    }
}

pub fn write(workspace: &Path, f: &ManagedFile) -> Result<String> {
    let mut s = String::from(
        "# 铺底/升级时写:哪些文件是 buddy 管的、什么版本、铺下去那一刻的内容\n\
         # 指纹。下次升级前先比对指纹:变了 = 人手改过,不覆盖只提示差异。\n",
    );
    for e in &f.files {
        s.push_str("\n[[file]]\n");
        s.push_str(&format!("path        = \"{}\"\n", e.path));
        s.push_str(&format!("version     = \"{}\"\n", e.version));
        s.push_str(&format!("fingerprint = \"{}\"\n", e.fingerprint));
    }
    write_file(workspace, REL_PATH, &s)?;
    Ok(s)
}
