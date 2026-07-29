//! plan/16 §1 S7 · Skill 正文的两个纯变换 —— 存库前的**剥离**与注入前的
//! **降级**。都是零 IO、wasm32-clean 的字符串函数,与 [`crate::skill_spec`]
//! 同层:规则在 skill_spec 判定,变换在这里执行,谁都不重抄一份。
//!
//! 为什么这两个函数必须共用一份:
//!
//! - **剥离**:`bw-app::skill_import` 从磁盘导入外部 skill 文件夹时,一直
//!   会把 SKILL.md 的 YAML frontmatter 切掉、只把 body 存进
//!   `SkillCard.content`(元数据已经进了 `name`/`descr` 两列)。而 BW 自带
//!   标准库的三件套走的是 `include_str!` 整文件,frontmatter 从没被切——
//!   于是同一个技能库里两种正文形态并存,详情面板顶上杵着 `--- name: … ---`,
//!   注入 prompt 时也把重复元数据一起喂给 agent。这是「展示没归一化」的
//!   真实缺口,修法只能是一份剥离逻辑两边共用。
//! - **降级**:规范正文以 `# 标题` 开头(SKILL.md 的既定形态,官方外库 33
//!   条真实如此)。但注入 prompt 时,正文是塞在 `## 技能(工作方法…)` 这个
//!   小节**之下**的——一个 H1 直接出现在 H2 小节里会把 prompt 的结构读乱。
//!   注入器因此负责把正文整体下沉,而不是反过来要求正文写成 `###` 迁就
//!   注入点(那会让展示端看到一份不合规范的正文——正是本轮要消灭的东西)。

/// 切掉正文开头的 YAML frontmatter 块(`---` 起、`---` 止),返回其后的
/// body(前导空行已去掉)。没有 frontmatter 就原样返回 trim 后的输入——
/// **绝不猜测**:只有第一行是 `---` 且后面真的有闭合 `---` 才算一个
/// frontmatter 块;闭合缺失说明这根本不是 frontmatter(可能是分隔线),
/// 原样保留,不吞用户内容。
///
/// 与 `bw-app::skill_import::split_frontmatter` 的关系:那边是导入路径的
/// **严格解析器**(缺 frontmatter 或未闭合都要报错,因为一个 SKILL.md 文件
/// 必须有元数据),这里是**宽容的清洗器**(没有就没有,不报错)——两者面对
/// 的输入不同,但对"什么算一个 frontmatter 块"的判定必须一致,故规则在此
/// 处以文档形式钉死:第一行 `---`,其后首个独占一行的 `---` 为闭合。
pub fn strip_frontmatter(raw: &str) -> String {
    let mut lines = raw.lines();
    match lines.next() {
        Some(first) if first.trim() == "---" => {}
        _ => return raw.trim_start_matches('\n').to_string(),
    }
    let mut body_start = None;
    let mut consumed = 1usize;
    for line in lines {
        consumed += 1;
        if line.trim() == "---" {
            body_start = Some(consumed);
            break;
        }
    }
    let Some(skip) = body_start else {
        // 未闭合 = 这不是 frontmatter,原样奉还(不吞内容)。
        return raw.to_string();
    };
    raw.lines()
        .skip(skip)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_start_matches('\n')
        .to_string()
}

/// plan/17 · 从 SKILL.md 原文的 frontmatter 块里**宽容地**取出单行
/// `description:` 的值——供内核侧需要一句话描述的投影使用(今天只有
/// [`crate::model::stage_workflow_with_playbook`] 的 `SkillRef.def`:desc 的
/// 正本随包文档进了 frontmatter,而 YAML 解析器不进内核)。
///
/// 与 bw-app 唯一严格解析器(serde_yaml)的关系,同 [`strip_frontmatter`]
/// 的约定:块判定规则一致(第一行 `---`,首个独占一行的 `---` 闭合,未闭合
/// = 不是 frontmatter);字段判定只认**顶格**的 `description:` 单行普通标量
/// (成对引号会剥掉)。块标量(`|`/`>`)、空值、缩进键一律诚实返回 `None`,
/// 绝不截半句装懂。bw-app 的 canon 构建器在 Boot 时用严格解析结果守卫这里
/// 的输出一致——两个实现谁漂移谁被当场抓住,而 vendored 文档因此被约束在
/// 这个宽容器读得懂的子集内。
pub fn frontmatter_description(raw: &str) -> Option<String> {
    let mut lines = raw.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut fm: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        fm.push(line);
    }
    if !closed {
        return None;
    }
    for line in fm {
        // 顶格键才算数:缩进的同名键属于某个嵌套映射,不是它。
        let Some(rest) = line.strip_prefix("description:") else {
            continue;
        };
        let v = rest.trim();
        if v.is_empty() || v.starts_with('|') || v.starts_with('>') {
            return None;
        }
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        return Some(v.to_string());
    }
    None
}

/// 把正文里每个 ATX markdown 标题下沉 `by` 级(`#` → `###` when `by == 2`),
/// 供注入器把一份规范正文嵌进 prompt 的某个小节之下。
///
/// 三条诚实约束:
/// - 只动**行首**的 `#` 序列且其后必须跟空格——`#hashtag`、正文中间的 `#`
///   都不是标题,不碰。
/// - 围栏代码块(``` 或 ~~~)内部一律不碰:代码里的 `# comment`(shell/
///   python/toml)不是标题,下沉它会改变代码语义。
/// - markdown 最深只到 H6,已经 6 级的标题保持 6 级(继续加 `#` 只会得到
///   一个不被任何渲染器认作标题的字符串)。
pub fn demote_headings(body: &str, by: usize) -> String {
    let mut in_fence = false;
    let mut fence_marker = "";
    body.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            for marker in ["```", "~~~"] {
                if trimmed.starts_with(marker) {
                    if !in_fence {
                        in_fence = true;
                        fence_marker = marker;
                    } else if fence_marker == marker {
                        in_fence = false;
                    }
                    return line.to_string();
                }
            }
            if in_fence {
                return line.to_string();
            }
            let hashes = line.chars().take_while(|c| *c == '#').count();
            if hashes == 0 || !line[hashes..].starts_with(' ') {
                return line.to_string();
            }
            let depth = (hashes + by).min(6);
            format!("{}{}", "#".repeat(depth), &line[hashes..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}
