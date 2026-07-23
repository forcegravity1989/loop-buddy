//! Real filesystem reading for `Command::ImportSkillPackage` (T2, plan/12
//! §2). Deliberately isolated in its own module and kept synchronous/`std`-
//! only: this is the one place in `bw-app` that reads real skill-folder
//! bytes off disk before handing parsed, owned data to the store layer.
//! `bw-core` must stay zero-IO/wasm32-compilable, so none of this can live
//! there — it belongs in a native, IO-doing crate, and `bw-app` (not
//! `bw-store`, which only knows SQL) is where `Command` handlers already do
//! this kind of real-world reading (see `LoadVersionLog`'s `git log` shellout
//! elsewhere in this crate).

use std::path::{Path, PathBuf};

/// One real skill folder, fully read off disk and ready to hand to
/// `bw_store::NewSkill` + `NewSkillFile` — copy-on-import: nothing here holds
/// a reference back to `source_path` after this function returns.
pub(crate) struct ParsedSkillPackage {
    pub name: String,
    pub desc: String,
    /// SKILL.md's own body (after the closing `---`), leading blank lines
    /// trimmed.
    pub content: String,
    /// Every other real file found recursively under the folder, as
    /// `(rel_path, content)` — `rel_path` uses `/` regardless of host OS,
    /// sorted for a deterministic import order.
    pub files: Vec<(String, String)>,
}

/// Read and parse a real skill folder. Fails honestly (no guessing) when:
/// `source_path` isn't a directory, it has no `SKILL.md`, the frontmatter
/// block is missing/unclosed, the frontmatter isn't valid YAML, it isn't a
/// mapping, or it's missing `name`/`description` — the two fields the
/// [agentskills.io spec](https://agentskills.io/specification) actually
/// requires. Every *other* frontmatter key (`disable-model-invocation`,
/// `argument-hint`, …) is read and silently ignored, on purpose — real
/// mattpocock/superpowers skills carry a mix of these and none of them are
/// this app's concern yet.
pub(crate) fn import_skill_package_from_disk(
    source_path: &str,
) -> Result<ParsedSkillPackage, String> {
    let dir = Path::new(source_path);
    if !dir.is_dir() {
        return Err(format!("source_path 不是一个存在的目录:{source_path}"));
    }
    let skill_md_path = dir.join("SKILL.md");
    if !skill_md_path.is_file() {
        return Err(format!(
            "{source_path} 下没有找到 SKILL.md,不符合 skill 文件夹约定"
        ));
    }
    let raw = std::fs::read_to_string(&skill_md_path).map_err(|e| {
        format!(
            "无法读取 {}:{e}(不是合法 UTF-8 文本,或权限不足)",
            skill_md_path.display()
        )
    })?;
    let (frontmatter, body) = split_frontmatter(&raw)?;
    let (name, desc) = parse_frontmatter_fields(&frontmatter)?;

    let mut files = Vec::new();
    collect_files(dir, dir, &skill_md_path, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(ParsedSkillPackage {
        name,
        desc,
        content: body,
        files,
    })
}

/// Split a SKILL.md's leading `---\n...\n---\n` YAML frontmatter block from
/// its body. Errors honestly if the file doesn't start with `---` or the
/// block is never closed, instead of silently treating the whole file as
/// body text.
fn split_frontmatter(raw: &str) -> Result<(String, String), String> {
    let mut lines = raw.lines();
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return Err("SKILL.md 缺少 YAML frontmatter(需以 --- 开头)".to_string()),
    }
    let mut fm_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        fm_lines.push(line);
    }
    if !closed {
        return Err("SKILL.md frontmatter 未闭合(缺少第二个 ---)".to_string());
    }
    let mut body = lines.collect::<Vec<_>>().join("\n");
    while let Some(stripped) = body.strip_prefix('\n') {
        body = stripped.to_string();
    }
    Ok((fm_lines.join("\n"), body))
}

/// Parse `name`/`description` out of a frontmatter YAML block via a real
/// YAML parser (`serde_yaml`) — correct on quoted strings, multi-line block
/// scalars, and any unknown key, rather than a fragile regex/line scan.
fn parse_frontmatter_fields(frontmatter: &str) -> Result<(String, String), String> {
    let value: serde_yaml::Value = serde_yaml::from_str(frontmatter)
        .map_err(|e| format!("SKILL.md frontmatter 不是合法 YAML:{e}"))?;
    let mapping = value
        .as_mapping()
        .ok_or_else(|| "SKILL.md frontmatter 顶层不是一个 YAML 映射".to_string())?;
    let name = mapping
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "SKILL.md frontmatter 缺少 name 字段".to_string())?;
    let description = mapping
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "SKILL.md frontmatter 缺少 description 字段".to_string())?;
    Ok((name, description))
}

/// Recursively collect every real file under `dir` (skipping `skip_path`,
/// i.e. SKILL.md itself), as `(rel_path, content)` relative to `base` with
/// forward slashes regardless of host OS.
fn collect_files(
    dir: &Path,
    base: &Path,
    skip_path: &Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("无法读取目录 {}:{e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取 {} 下的目录项失败:{e}", dir.display()))?;
        let path: PathBuf = entry.path();
        if path == skip_path {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, base, skip_path, out)?;
        } else if path.is_file() {
            let rel_path = path
                .strip_prefix(base)
                .map_err(|_| format!("无法计算相对路径:{}", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path).map_err(|e| {
                format!(
                    "无法读取 {}:{e}(不是合法 UTF-8 文本,或权限不足)",
                    path.display()
                )
            })?;
            out.push((rel_path, content));
        }
    }
    Ok(())
}
