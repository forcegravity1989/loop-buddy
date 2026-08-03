//! Real filesystem reading for `Command::ImportSkillPackage` (T2, plan/12
//! §2) and `Command::ImportSkillLibrary` (T3, plan/12 §1/§2). Deliberately
//! isolated in its own module and kept synchronous/`std`-only: this is the
//! one place in `bw-app` that reads real skill-folder bytes off disk before
//! handing parsed, owned data to the store layer. `bw-core` must stay zero-
//! IO/wasm32-compilable, so none of this can live there — it belongs in a
//! native, IO-doing crate, and `bw-app` (not `bw-store`, which only knows
//! SQL) is where `Command` handlers already do this kind of real-world
//! reading (see `LoadVersionLog`'s `git log` shellout elsewhere in this
//! crate).

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

/// One parsed `SKILL.md` document: strict frontmatter `name`/`description` +
/// the body after the closing `---` (leading blank lines trimmed).
pub(crate) struct ParsedSkillMd {
    pub name: String,
    pub desc: String,
    pub body: String,
    /// The optional frontmatter `category` — read so `crate::bw_canon` can
    /// guard it against the kind-derived value (plan/17: a key the vendored
    /// docs carry must not be a silent no-op). External imports keep their
    /// own category logic and ignore this.
    pub category: Option<String>,
}

/// plan/17 · **the** one SKILL.md text parser — the disk import path
/// ([`import_skill_package_from_disk`]) and the bw-standard canon builder
/// (`crate::bw_canon`, over `bw_core::bw_library`'s vendored docs) both go
/// through here, so 装载系统只有一条:SKILL.md 文档 → 解析 → 行. Takes raw
/// text, not a path: where the bytes come from (disk vs `include_str!`) is
/// the caller's business.
pub(crate) fn parse_skill_md(raw: &str) -> Result<ParsedSkillMd, String> {
    let (frontmatter, body) = split_frontmatter(raw)?;
    let (name, desc) = parse_frontmatter_fields(&frontmatter)?;
    let category = parse_optional_frontmatter_str(&frontmatter, "category")?;
    Ok(ParsedSkillMd {
        name,
        desc,
        body,
        category,
    })
}

/// Read one optional string key out of an already-split frontmatter block,
/// through the same `serde_yaml` parser the required fields use. `Ok(None)`
/// = the key is absent (legitimate); `Err` = it is present but not a string
/// (a malformed doc, never silently ignored).
fn parse_optional_frontmatter_str(frontmatter: &str, key: &str) -> Result<Option<String>, String> {
    let value: serde_yaml::Value = serde_yaml::from_str(frontmatter)
        .map_err(|e| format!("SKILL.md frontmatter 不是合法 YAML:{e}"))?;
    let Some(mapping) = value.as_mapping() else {
        return Ok(None);
    };
    match mapping.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("SKILL.md frontmatter 的 {key} 不是字符串")),
    }
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
    let parsed = parse_skill_md(&raw)?;

    let mut files = Vec::new();
    collect_files(dir, dir, &skill_md_path, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(ParsedSkillPackage {
        name: parsed.name,
        desc: parsed.desc,
        content: parsed.body,
        files,
    })
}

/// plan/渠道6: scan a project workspace's own `skills/` dir for skill
/// packages — each a folder with a `SKILL.md` (+ optional support files),
/// the same shape `find_skill_package_dirs` + `import_skill_package_from_disk`
/// already handle for external libraries. `project_id`/`source` are filled by
/// the caller (`App::sync_project_assets`), not here — this helper stays
/// pure-IO, matching `import_skill_package_from_disk`.
///
/// Soft-degrade by design (a project without its own `skills/` is normal, not
/// an error): `workspace/skills/` missing → empty vec; a single package that
/// fails to parse is skipped, not fatal — one malformed folder doesn't sink
/// the rest, and the project's good skills still register.
pub(crate) fn scan_project_skills_dir(workspace: &str) -> Vec<ParsedSkillPackage> {
    let root = Path::new(workspace).join("skills");
    match find_skill_package_dirs(&root.to_string_lossy()) {
        Ok(dirs) => dirs
            .iter()
            .filter_map(|d| import_skill_package_from_disk(&d.to_string_lossy()).ok())
            .collect(),
        Err(_) => Vec::new(),
    }
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

/// Recursively find every directory under `root_path` that directly contains
/// a `SKILL.md` — a "skill package" per plan/12 §2's convention — for T3's
/// batch `Command::ImportSkillLibrary`. Directories named `node_modules`,
/// `.git`, or `target` are pruned without descending: a purely defensive/
/// efficiency guard (real skill libraries don't nest `SKILL.md` inside any of
/// these; they'd be skipped anyway for lacking one), not a hardcoded skill
/// convention. Once a directory is recognised as a skill package it stops
/// descending into its own subdirectories — real mattpocock/superpowers
/// skills never nest a second `SKILL.md` inside an already-matched skill
/// folder, and taking the shallowest match keeps "one directory = one skill"
/// unambiguous if that ever changed. Returned paths are sorted for a
/// deterministic import order.
pub(crate) fn find_skill_package_dirs(root_path: &str) -> Result<Vec<PathBuf>, String> {
    let root = Path::new(root_path);
    if !root.is_dir() {
        return Err(format!("root_path 不是一个存在的目录:{root_path}"));
    }
    let mut out = Vec::new();
    walk_for_skill_dirs(root, &mut out)?;
    out.sort();
    Ok(out)
}

/// Pruned directory names: never descended into while searching for
/// `SKILL.md` folders. None of the two real libraries this app imports
/// (mattpocock-skills, superpowers) contain any of these, but a library root
/// pointed at an arbitrary checkout could — pruning here is cheap insurance,
/// not a claim that real skill data lives here today.
const PRUNED_DIR_NAMES: [&str; 3] = ["node_modules", ".git", "target"];

fn walk_for_skill_dirs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if dir.join("SKILL.md").is_file() {
        out.push(dir.to_path_buf());
        // Shallowest match wins — don't look for a second SKILL.md nested
        // underneath this one.
        return Ok(());
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("无法读取目录 {}:{e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取 {} 下的目录项失败:{e}", dir.display()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if PRUNED_DIR_NAMES.contains(&name) {
            continue;
        }
        walk_for_skill_dirs(&path, out)?;
    }
    Ok(())
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
