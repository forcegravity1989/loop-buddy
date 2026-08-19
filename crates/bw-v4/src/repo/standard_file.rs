//! `.bw/standard.toml` —— 这个项目用的是哪版规范、铺了哪几类件。
//!
//! 与 `standard/VERSION`(buddy 二进制自带哪版)是两件事:这份记「项目侧」,
//! 那个记「buddy 侧」,数值常相等但含义不同,不合并成一个字段。

use super::{parse_toml, read_to_string, write_file, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REL_PATH: &str = ".bw/standard.toml";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StandardFile {
    pub version: String,
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    /// `"builtin"` = buddy 自带那套;将来同事贡献的扩展件会有别的取值。
    #[serde(default)]
    pub source: String,
}

pub fn parse(raw: &str) -> Result<StandardFile> {
    parse_toml(REL_PATH, raw)
}

/// 读到就说明有人先接过这个项目(后来者接入):不重新建活,读正本预填。
pub fn read(workspace: &Path) -> Result<Option<StandardFile>> {
    match read_to_string(workspace, REL_PATH)? {
        None => Ok(None),
        Some(raw) => parse(&raw).map(Some),
    }
}

pub fn write(workspace: &Path, f: &StandardFile) -> Result<String> {
    let q = super::toml_string;
    let arr = |v: &[String]| {
        let items: Vec<String> = v.iter().map(|x| q(x)).collect();
        format!("[{}]", items.join(", "))
    };
    let body = format!(
        "# 这个项目用的是哪版规范、铺了哪几类件。buddy 二进制自带哪版记在\n\
         # buddy 侧的 standard/VERSION 里,是另一件事,不要混。\n\
         version    = {}\n\
         enabled    = {}\n\
         extensions = {}\n\
         source     = {}\n",
        q(&f.version),
        arr(&f.enabled),
        arr(&f.extensions),
        q(&f.source)
    );
    write_file(workspace, REL_PATH, &body)?;
    Ok(body)
}
