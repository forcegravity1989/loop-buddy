//! plan/16 §2 防线 4 · Skill 标准规范审计指挥器。
//!
//! 对任意 DB 全量跑 `bw_core::skill_spec` 机检(与命令层守卫 / SkillHub 徽记
//! 同一份检查函数,不另立口径),打印违规报告;`--fix` 只做规范 §4 台账里的
//! **确定性校正**,幂等可重跑:
//!
//! 1. 先走真实 `Command::Boot`——bw-standard 库(playbook 五 + 标配三)的
//!    pristine 升源与 desc+content 自愈对账发生在产品自己的 Boot 路径里,
//!    本指挥器不重抄一份逻辑,只负责触发与读回。
//! 2. `--fix` 再执行台账内置的两条改名映射(中文名自建技能 → kebab-case,
//!    见 [`RENAMES`]——评审过的固定台账,不是运行时编造),走真实
//!    `Command::UpdateSkill`(S1 守卫本身把关新名),并同步更新按名引用
//!    旧技能名的 agent(`Command::UpdateAgent`)。
//! 3. S2(重名)与 S6(存储层 source 编码一致)是全库/原始列级检查,纯
//!    per-skill 函数看不见——在这里用 sqlx 原始读补上(与
//!    `build_aihot_fixture` 的先例一致:一次性维护动作直接 reach for sqlx)。
//!
//! **应跑在真实日常库的一次性副本上**(与 `migrate_legacy_db` 同一纪律):
//!   cp "~/Library/Application Support/BuildersWorkbench/workbench.db" /tmp/audit.db
//!   cargo run -p bw-app --example audit_skills -- /tmp/audit.db --fix
//! 之后 `sqlite3 /tmp/audit.db "SELECT name, source, official_library FROM skill"`
//! 独立复核——报告不代答,读回为证。

use bw_app::{App, Command};
use bw_core::model::HubSource;
use bw_core::skill_spec::{check_skill, must_fix_count, SkillSpecInput, SpecSeverity};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::collections::HashMap;
use std::sync::Arc;

/// plan/16 §4 校正台账:两条既有中文名自建技能的改名映射。
/// (旧名, 新名, 新 desc——补齐 S4「适用:…」触发段,中文原名留痕)。
const RENAMES: &[(&str, &str, &str)] = &[
    (
        "关键词关注面打分法",
        "keyword-focus-scoring",
        "按用户配置的关注面关键词给抓取条目打分,分数不够不上日报——0 分零容忍,不为凑量降门槛。适用:日报编辑筛选抓取条目,或任何按关键词相关性过滤内容源的活(原名「关键词关注面打分法」)",
    ),
    (
        "多源体量控制法",
        "per-source-volume-cap",
        "多来源合并输出限量时必须按源分别限(cap_per_source),绝不对合并列表整体截断——量大的源会挤占量小的源。适用:聚合多个来源出日报/榜单等限量输出的实现与评审(蒸馏自真实修复,原名「多源体量控制法」)",
    ),
];

fn spec_input<'a>(s: &'a bw_core::model::SkillCard) -> SkillSpecInput<'a> {
    let official_import = matches!(
        &s.source,
        HubSource::Official { official_library } if official_library != "bw-standard"
    );
    SkillSpecInput {
        name: &s.name,
        desc: &s.desc,
        content: &s.content,
        official_import,
        distilled: s.distilled_from_issue.is_some(),
        has_origin_agent: s.origin_agent.is_some(),
    }
}

/// 全量机检 + S2 重名扫描,打印报告,返回 (硬规违规技能数, 提示技能数)。
async fn report(store: &Arc<dyn Store>, banner: &str) -> (usize, usize) {
    let skills = store.list_skills().await.unwrap();
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for s in &skills {
        *name_counts.entry(s.name.as_str()).or_default() += 1;
    }

    let mut must_fix_skills = 0usize;
    let mut advisory_skills = 0usize;
    println!("---------------- {banner} ----------------");
    for s in &skills {
        let mut findings = check_skill(spec_input(s));
        if name_counts[s.name.as_str()] > 1 {
            findings.push(bw_core::skill_spec::SpecFinding {
                rule: "S2",
                severity: SpecSeverity::MustFix,
                message: format!(
                    "名称「{}」在库内重复 {} 次",
                    s.name,
                    name_counts[s.name.as_str()]
                ),
            });
        }
        if findings.is_empty() {
            continue; // 合规不出声——报告只列有事的行。
        }
        let n_must = must_fix_count(&findings);
        if n_must > 0 {
            must_fix_skills += 1;
        } else {
            advisory_skills += 1;
        }
        let lib = match &s.source {
            HubSource::Official { official_library } => format!("official/{official_library}"),
            other => format!("{:?}", other).to_lowercase(),
        };
        println!("· {} [{}]", s.name, lib);
        for f in findings {
            let tag = match f.severity {
                SpecSeverity::MustFix => "待校正",
                SpecSeverity::Advisory => "提示",
            };
            println!("    {} {} — {}", f.rule, tag, f.message);
        }
    }
    println!(
        "skills={} · 有待校正项的技能={} · 仅提示的技能={}",
        skills.len(),
        must_fix_skills,
        advisory_skills
    );
    (must_fix_skills, advisory_skills)
}

/// S6 · 存储层编码一致性:`source='official'` ⟺ `official_library` 非空。
/// 原始列级检查,须绕过 `parse_hub_source` 的宽容读回。
async fn s6_raw_scan(db_path: &str) -> Vec<(String, String, String)> {
    use sqlx::Row;
    let url = format!("sqlite://{db_path}");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let rows = sqlx::query(
        "SELECT name, source, official_library FROM skill
         WHERE (source='official' AND official_library='')
            OR (source<>'official' AND official_library<>'' AND source<>'self_built')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // 注:source<>'official' 且 library 非空、且 source='self_built' 的行是
    // T11「改编自」的合法留痕,不算违规,上面的 WHERE 已放行。
    rows.iter()
        .map(|r| {
            (
                r.get::<String, _>(0),
                r.get::<String, _>(1),
                r.get::<String, _>(2),
            )
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| {
        eprintln!("usage: audit_skills <db-path>(真实库的一次性副本,绝不指原件) [--fix]");
        std::process::exit(2);
    });
    let fix = args.next().as_deref() == Some("--fix");

    println!("================ plan/16 · Skill 规范审计 ================");
    println!("db: {db_path} · fix: {fix}");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    report(&store, "Boot 前(原始现状)").await;
    let s6_before = s6_raw_scan(&db_path).await;
    println!("S6 存储层编码不一致(Boot 前): {} 行", s6_before.len());
    for (n, s, l) in &s6_before {
        println!("    · {n} source={s} official_library={l:?}");
    }

    // ── 真实 Boot:bw-standard pristine 升源 + desc/content 自愈对账 ──
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    if fix {
        let skills = store.list_skills().await.unwrap();
        let agents = store.list_agents().await.unwrap();
        for (old_name, new_name, new_desc) in RENAMES {
            let Some(s) = skills.iter().find(|s| s.name == *old_name) else {
                println!("fix · 「{old_name}」不在库中(已改名或不存在),跳过");
                continue;
            };
            if skills.iter().any(|x| x.name == *new_name) {
                println!("fix · 目标名「{new_name}」已被占用,拒绝改名(人工裁决)");
                continue;
            }
            app.dispatch(Command::UpdateSkill {
                id: s.id,
                name: new_name.to_string(),
                desc: new_desc.to_string(),
                category: s.category.clone(),
                content: s.content.clone(),
            })
            .await
            .unwrap();
            println!("fix · 「{old_name}」 → 「{new_name}」(desc 补「适用」触发段)");

            // 按名引用旧技能名的 agent 同步改引,联合键不留悬空。
            for a in agents
                .iter()
                .filter(|a| a.skills.iter().any(|k| k.name == *old_name))
            {
                let new_skills: Vec<String> = a
                    .skills
                    .iter()
                    .map(|k| {
                        if k.name == *old_name {
                            new_name.to_string()
                        } else {
                            k.name.clone()
                        }
                    })
                    .collect();
                app.dispatch(Command::UpdateAgent {
                    id: a.id,
                    name: a.name.clone(),
                    role: a.role.clone(),
                    skills: new_skills,
                    model: a.model.clone(),
                    instructions: a.instructions.clone(),
                })
                .await
                .unwrap();
                println!("fix · agent「{}」的技能引用同步改为「{new_name}」", a.name);
            }
        }
    }

    let (must_fix_after, _) = report(
        &store,
        if fix {
            "Boot+fix 后"
        } else {
            "Boot 后(仅自愈,未 --fix)"
        },
    )
    .await;
    let s6_after = s6_raw_scan(&db_path).await;
    println!("S6 存储层编码不一致(现在): {} 行", s6_after.len());
    for (n, s, l) in &s6_after {
        println!("    · {n} source={s} official_library={l:?}");
    }

    println!("独立复核: sqlite3 '{db_path}' \"SELECT name, source, official_library, substr(descr,1,40) FROM skill ORDER BY source, name\"");
    if fix && (must_fix_after > 0 || !s6_after.is_empty()) {
        println!("结论:--fix 后仍有待校正项(见上),不装全绿。");
        std::process::exit(1);
    }
}
