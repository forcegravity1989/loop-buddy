//! Real filesystem reading for `Command::ImportAgentDefinition` (T5, plan/12
//! §3). Parallel structure to `skill_import.rs` (T2): isolated in its own
//! module, synchronous/`std`-only — the one place in `bw-app` that reads a
//! real AGENT.md's bytes off disk before handing parsed, owned data to the
//! store layer. `bw-core` must stay zero-IO/wasm32-compilable, so none of
//! this can live there.
//!
//! Unlike a skill folder, "Agent" == AGENT.md is a **single file** — no
//! sibling files to collect, no `skill_file`-equivalent table. The real ECC
//! (everything-claude-code) subagent format this parses:
//!
//! ```text
//! ---
//! name: a11y-architect
//! description: Accessibility Architect specializing in WCAG 2.2 …
//! model: sonnet
//! tools: ["Read", "Write", "Edit", "Grep", "Glob"]
//! ---
//!
//! ## Prompt Defense Baseline
//! …
//! ```

use std::path::Path;

/// One real AGENT.md, fully read off disk and ready to hand to
/// `bw_store::NewAgent` — copy-on-import: nothing here holds a reference back
/// to `source_path` after this function returns.
pub(crate) struct ParsedAgentDefinition {
    pub name: String,
    /// Frontmatter `description` — maps onto `AgentCard.role` (the existing
    /// free-text "what this agent does" field every other Agent-creation path
    /// already populates that way, e.g. `CreateAgentForm`'s "角色描述").
    pub description: String,
    /// Frontmatter `tools` — AllowedTools. `[]` if the key is absent (no
    /// restriction declared), never invented.
    pub tools: Vec<String>,
    /// Frontmatter `model` (e.g. `sonnet`/`opus`) — an honest label, same as
    /// every other Agent row's `model` field; `""` if absent.
    pub model: String,
    /// The body after the closing `---`, leading blank lines trimmed —
    /// becomes `AgentCard.instructions`.
    pub instructions: String,
}

/// Read and parse a real AGENT.md file. Fails honestly (no guessing) when:
/// `source_path` isn't a file, the frontmatter block is missing/unclosed,
/// the frontmatter isn't valid YAML, it isn't a mapping, it's missing
/// `name`/`description`, or `tools`/`model` are present but the wrong shape
/// (`tools` not an array of strings, `model` not a string). Every *other*
/// frontmatter key (ECC's `color`, …) is read and silently ignored, on
/// purpose — real ECC agent files carry a handful of these and none of them
/// are this app's concern yet.
pub(crate) fn import_agent_definition_from_disk(
    source_path: &str,
) -> Result<ParsedAgentDefinition, String> {
    let path = Path::new(source_path);
    if !path.is_file() {
        return Err(format!("source_path 不是一个存在的文件:{source_path}"));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "无法读取 {}:{e}(不是合法 UTF-8 文本,或权限不足)",
            path.display()
        )
    })?;
    let (frontmatter, body) = split_frontmatter(&raw)?;
    let (name, description, tools, model) = parse_frontmatter_fields(&frontmatter)?;

    Ok(ParsedAgentDefinition {
        name,
        description,
        tools,
        model,
        instructions: body,
    })
}

/// Split an AGENT.md's leading `---\n...\n---\n` YAML frontmatter block from
/// its body. Errors honestly if the file doesn't start with `---` or the
/// block is never closed, instead of silently treating the whole file as
/// body text. Identical rule to `skill_import::split_frontmatter` — kept as
/// its own copy rather than a shared helper, matching this crate's existing
/// one-module-per-import-kind structure.
fn split_frontmatter(raw: &str) -> Result<(String, String), String> {
    let mut lines = raw.lines();
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return Err("AGENT.md 缺少 YAML frontmatter(需以 --- 开头)".to_string()),
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
        return Err("AGENT.md frontmatter 未闭合(缺少第二个 ---)".to_string());
    }
    let mut body = lines.collect::<Vec<_>>().join("\n");
    while let Some(stripped) = body.strip_prefix('\n') {
        body = stripped.to_string();
    }
    Ok((fm_lines.join("\n"), body))
}

/// Parse `name`/`description`(required) + `tools`/`model`(optional) out of a
/// frontmatter YAML block via a real YAML parser (`serde_yaml`) — correct on
/// quoted strings (ECC's GAN-harness agents wrap `description` in `"…"` when
/// it contains a colon), flow-style arrays (`tools: ["Read", "Grep"]`, the
/// real ECC shape), and any unknown key (`color: purple`, ignored).
fn parse_frontmatter_fields(
    frontmatter: &str,
) -> Result<(String, String, Vec<String>, String), String> {
    let value: serde_yaml::Value = serde_yaml::from_str(frontmatter)
        .map_err(|e| format!("AGENT.md frontmatter 不是合法 YAML:{e}"))?;
    let mapping = value
        .as_mapping()
        .ok_or_else(|| "AGENT.md frontmatter 顶层不是一个 YAML 映射".to_string())?;
    let name = mapping
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "AGENT.md frontmatter 缺少 name 字段".to_string())?;
    let description = mapping
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "AGENT.md frontmatter 缺少 description 字段".to_string())?;
    let tools = match mapping.get("tools") {
        None => Vec::new(),
        Some(v) => {
            let seq = v
                .as_sequence()
                .ok_or_else(|| "AGENT.md frontmatter 的 tools 不是一个数组".to_string())?;
            seq.iter()
                .map(|t| {
                    t.as_str().map(str::to_string).ok_or_else(|| {
                        "AGENT.md frontmatter 的 tools 数组元素不是字符串".to_string()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    let model = match mapping.get("model") {
        None => String::new(),
        Some(v) => v
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "AGENT.md frontmatter 的 model 不是字符串".to_string())?,
    };
    Ok((name, description, tools, model))
}
