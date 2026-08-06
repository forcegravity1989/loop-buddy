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
use bw_core::skill_spec::{check_skill_card, must_fix_count, SpecSeverity};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::collections::HashMap;
use std::sync::Arc;

/// plan/16 §4 校正台账:两条 aihot 项目自建/蒸馏技能的**确定性**校正——
/// 改名(S1)+ desc 补触发段(S4)+ 正文归一为规范 SKILL 形态(S7)。
///
/// 三条纪律钉在这里,不在运行时临场发挥:
/// - **正文是重排,不是重写**:原正文的每一条步骤原样保留(措辞不动),只补
///   `# 标题` / `## 何时用` / `## 反例` 的结构外壳——把「一段裸提示词」变成
///   一份合规 SKILL 正文,不新增任何原文没有的做法主张。
/// - **悬空引用如实删掉**:原 `keyword-focus-scoring` 正文里的「(见去重技
///   能)」指向一个技能库里**不存在**的技能(SQL 核过:无任何 去重/dedup 命名
///   的行)——最佳实践要求引用一级可达,一个指不到东西的指针不如没有。
/// - **蒸馏溯源不碰**:`per-source-volume-cap` 是从真实 Issue 蒸馏来的,
///   `distilled_from_issue`/`origin_agent`/`uses` 全部保持原值(`SkillEdit`
///   根本没有这些字段,结构上碰不到)。
struct Correction {
    old_name: &'static str,
    new_name: &'static str,
    new_desc: &'static str,
    new_content: &'static str,
}

const CORRECTIONS: &[Correction] = &[
    Correction {
        old_name: "关键词关注面打分法",
        new_name: "keyword-focus-scoring",
        new_desc: "按用户配置的关注面关键词给抓取条目打分,分数不够不上日报——0 分零容忍,不为凑量降门槛。适用:日报编辑筛选抓取条目,或任何按关键词相关性过滤内容源的活(原名「关键词关注面打分法」)",
        new_content: "# 关键词关注面打分法 (keyword-focus-scoring)\n\
             \n\
             ## 何时用\n\
             \n\
             aihot 日报(或任何按关注面筛选内容源的项目)决定「哪些抓取条目够格\
             上日报」时。\n\
             \n\
             ## 步骤\n\
             \n\
             1. 读 config.json 的 keywords 列表(用户真实配置的关注面,不是猜的)。\n\
             2. 对每条真实抓取到的标题/摘要,逐关键词做子串匹配(忽略大小写),\
             命中数 = 分数。\n\
             3. 分数为 0 的条目不上日报——没有例外,不为了「凑够数量」降低门槛。\n\
             4. 命中多个关键词的条目排在日报前面(分数降序)。\n\
             5. 同一天多条命中同一实际事件的,去重只留一条,不是「都留着凑数」。\n\
             \n\
             ## 反例\n\
             \n\
             为了让今天的日报「看起来有内容」而放宽 0 分门槛——那是把噪音当产出,\
             读者下次就不信这份日报了。",
    },
    Correction {
        old_name: "多源体量控制法",
        new_name: "per-source-volume-cap",
        new_desc: "多来源合并输出限量时必须按源分别限(cap_per_source),绝不对合并列表整体截断——量大的源会挤占量小的源。适用:聚合多个来源出日报/榜单等限量输出的实现与评审(蒸馏自真实修复,原名「多源体量控制法」)",
        new_content: "# 多源体量控制法 (per-source-volume-cap)\n\
             \n\
             ## 何时用\n\
             \n\
             多个来源合并成一份限量输出(日报/榜单/摘要)时——设计阶段与评审阶段\
             都适用。本技能蒸馏自本项目真实修复 #11。\n\
             \n\
             ## 步骤\n\
             \n\
             1. 多个来源合并后按分数/时间统一排序前,先想清楚:排序对合并结果\
             **没有来源公平性**——量大的来源天然会挤占量小的来源,即便后者同样相关。\n\
             2. 因此「限量」必须**按来源分别限**(cap_per_source),不是对合并后的\
             列表整体截断(`items[:N]`)——后者是本项目真实踩过的坑。\n\
             3. 先用真实数据测量「截断前 vs 截断后」每个来源各剩多少条,确认改动\
             方向对(见 docs/regression.md 的方法),而不是改完就假设它对了。\n\
             4. 时间维度的重复浪费(如同时抓 topstories+newstories)优先合并同类项\
             而不是加并发数掩盖——先问「这两份真的都要吗」,再问「怎么让它更快」。\n\
             \n\
             ## 反例\n\
             \n\
             对合并列表整体 `items[:N]`——这正是 #11 的原始 bug:小来源被大来源挤到\
             截断线以下,输出看起来「有量」,覆盖面却丢了。",
    },
];

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
        let mut findings = check_skill_card(s);
        if name_counts[s.name.as_str()] > 1 {
            // 分域同 per-skill 规则:重名组里只要有一条 BW 自产,就是硬规
            // 违规;全组皆官方外库导入则如实降为提示(原文不改写)。
            let any_bw_authored = skills
                .iter()
                .filter(|x| x.name == s.name)
                .any(|x| !x.source.is_external_official());
            findings.push(bw_core::skill_spec::SpecFinding {
                rule: "S2",
                severity: if any_bw_authored {
                    SpecSeverity::MustFix
                } else {
                    SpecSeverity::Advisory
                },
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
        for c in CORRECTIONS {
            let (old_name, new_name) = (c.old_name, c.new_name);
            // 认「改名前」也认「已改名」——台账要能在一条已经改过名、但正文
            // 还没归一的行上继续把剩下的校正做完(真实日常库正是这个状态:
            // 改名先落了一轮,S7 正文规则是后来才有的)。
            let Some(s) = skills
                .iter()
                .find(|s| s.name == old_name || s.name == new_name)
            else {
                println!("fix · 「{old_name}」不在库中(已改名或不存在),跳过");
                continue;
            };
            if skills.iter().any(|x| x.name == new_name && x.id != s.id) {
                println!("fix · 目标名「{new_name}」已被别的行占用,拒绝改名(人工裁决)");
                continue;
            }
            if s.name == new_name && s.desc == c.new_desc && s.content == c.new_content {
                println!("fix · 「{new_name}」已合规,零操作");
                continue;
            }
            app.dispatch(Command::UpdateSkill {
                id: s.id,
                name: new_name.to_string(),
                desc: c.new_desc.to_string(),
                category: s.category.clone(),
                content: c.new_content.to_string(),
                stages: None,
            })
            .await
            .unwrap();
            println!(
                "fix · 「{}」→「{new_name}」:desc 补触发段(S4)+ 正文归一为规范 SKILL 形态(S7,步骤原样保留)",
                s.name
            );

            // 按名引用旧技能名的队友同步改引,联合键不留悬空。
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
                println!("fix · 队友「{}」的技能引用同步改为「{new_name}」", a.name);
            }
        }
    }

    if fix {
        // S6 顽固行校正:Boot 的 pristine 升源只收编「与正本逐字一致」的
        // 行;正文被人改过的旧编码行(source='official' 且空库名)诚实留在
        // 自建域,但原始列还挂着 official——把原始编码归一为 self_built,
        // 与 parse_hub_source 早已在读的语义完全一致,零行为变化,纯编码
        // 卫生(plan/16 §4 台账)。
        use sqlx::Row as _;
        let url = format!("sqlite://{db_path}");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        let n = sqlx::query(
            "UPDATE skill SET source='self_built', updated_at=unixepoch(), rev=rev+1
             WHERE source='official' AND official_library=''",
        )
        .execute(&pool)
        .await
        .unwrap()
        .rows_affected();
        if n > 0 {
            println!("fix · S6 顽固行原始编码归一 self_built:{n} 行(语义不变,纯卫生)");
        }
        let _ = pool;
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
