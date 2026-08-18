//! `App::dispatch`:Command → 用例的唯一路由。一个大 match,按臂提取见 docs/BACKLOG.md 第 9 条。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    pub async fn dispatch(&mut self, cmd: Command) -> Result<(), AppError> {
        match cmd {
            Command::Boot => {
                // V1-TermRefactor4: Boot 只重算信号 / 播种 / 对账 —— **绝不**
                // 批量 spawn 历史 claude 会话。重启后 PTY 如实全死;用户点哪张
                // 卡才按该卡 `--resume`(见 open_conversation /
                // run_issue_interactive)。
                // Staleness is clock-relative: what was green last week may be
                // amber-capped today. Re-derive every running project on boot so
                // the wall never shows a stale cache as fresh truth.
                let projects = self.store.list_projects().await?;
                for p in &projects {
                    if p.phase == Readiness::Running {
                        self.store.recompute_signals(p.id, now()).await?;
                    }
                }
                self.refresh_projects().await?;
                // Real OMC/ECC catalog, not fabricated sample data — a no-op
                // once the hub tables are non-empty (checked inside).
                bw_store::seed_hub_if_empty(self.store.as_ref()).await?;
                // plan/17 · 装载统一: the bw-standard skill library (five
                // stage working-method skills + the standard-issue trio) is
                // parsed from its vendored package documents
                // (`bw_core::bw_library` — the real docs/skills/<slug>/
                // SKILL.md files compiled in) through THE import parser
                // (`bw_canon`, name==slug + kernel-desc guards inside), so
                // BW's own skills and every external import enter through
                // one loading chain. Seeding is by-name idempotent (an
                // already-seeded database gains missing rows too).
                //
                // A malformed vendored doc must NOT `?` out here. 桌面壳
                // (`kernel.rs`)把 Boot 的 Err 吞成一条 toast 后照常开窗,
                // 所以提前返回不是「拒绝启动」,而是静默跳过它后面的全部
                // 初始化(五角色 agent 播种、阶段吞吐指标回填、workflow/
                // skill 刷新)——用户看到的会是一个空 Hub 加一条转瞬的提
                // 示。改为:canon 坏了就跳过 bw-standard 的播种与对账(空
                // canon 让下面几个循环自然 no-op),其余初始化照跑,错误
                // 留到 Boot 末尾原样抛出。
                let (skill_canon, canon_err) = match crate::bw_canon::bw_standard_skill_canon() {
                    Ok(canon) => (canon, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                bw_store::seed_bw_standard_skills_if_missing(self.store.as_ref(), &skill_canon)
                    .await?;
                // The five stage-role agents (bw_core::playbook projections)
                // — by-name idempotent, so an already-seeded database gains
                // them too.
                bw_store::seed_stage_role_agents_if_missing(self.store.as_ref()).await?;
                // P8 (2026-07-28, widened by plan/16 §2): the bw-standard
                // skill library (issue trio + five playbook stage skills) is
                // reconciled — `desc`+`content` — against its canon (plan/17
                // 起 = the parsed vendored package documents, `skill_canon`
                // above) on every Boot, unconditionally — not gated behind a
                // one-time migration flag. `seed_bw_standard_skills_if_missing`
                // above is by-name idempotent — it only ever plants a fresh
                // row, never overwrites an existing one (that function's own
                // doc comment: "内容对账是 Pass 2,不是重新 seed") — so
                // an existing row on an older database never picks up a
                // later edit to the SKILL.md source on its own. P2
                // originally closed that gap with a one-shot
                // `STANDARD_SKILL_CONTENT_REFRESH_DONE_KEY` app_meta guard,
                // but that shape is a treadmill: every future SKILL.md edit
                // would need its own new guard key (`_v2`, `_v3`, …) to ever
                // reach existing databases. Replaced with unconditional
                // reconciliation instead, because the invariant licensing it
                // is permanent, not one-time: a `skill` row whose
                // `source == Official { official_library: "bw-standard" }`
                // is *by definition* stale the moment its `content` diverges
                // from the compiled-in SKILL.md text — the instant a human
                // edits that row, T11 ("编辑即脱离源头") flips it to
                // `SelfBuilt` and it stops being a `bw-standard` row at all.
                // So there is no legitimate case of an `Official
                // { official_library: "bw-standard" }` row intentionally
                // holding content that differs from the source file — no
                // divergence worth preserving, nothing to gate. Every Boot
                // re-diffs and, only when different, overwrites. Cost is
                // negligible: `list_skills()` is a call this same Boot path
                // already makes right above for
                // `seed_bw_standard_skills_if_missing`.
                // The `STANDARD_SKILL_CONTENT_REFRESH_DONE_KEY` app_meta
                // guard this replaced is gone from `legacy_migration.rs`
                // entirely (see that module's own historical-note comment at
                // the old declaration site); the stray `app_meta` row it
                // left behind in any database that already ran P2 is
                // harmless and deliberately not migrated away (no
                // destructive DELETE for a row nothing reads anymore).
                {
                    // plan/16 §2 防线 2, plan/17 合流: the reconciled canon
                    // is the whole bw-standard library — `skill_canon`, the
                    // same parsed-package-document rows the seed above ate
                    // (装载与对账不再各建一份) — and the diff covers `desc`
                    // as well as `content` (a canonical description fix must
                    // reach existing rows the same way a SKILL.md edit
                    // does). `CanonicalSkill.content` is the parser's body
                    // output, frontmatter already gone (plan/16 S7), so what
                    // Boot compares against is exactly what the hub should
                    // store and show.

                    // The five playbook skills' pre-plan/16 canonical descs —
                    // a *bounded* migration aid, not a version treadmill:
                    // Pass 1 only ever matters for the pre-plan/16 row
                    // population (fresh seeds are born `Official`), and that
                    // population's canonical descs are fixed history. A row
                    // whose desc is neither today's canon nor one of these
                    // is a real user edit and must not be promoted.
                    const LEGACY_CANON_DESCS: [&str; 5] = [
                        "证据先行:只写站得住的内容,标注未核实",
                        "规格即测试:每条验收标准落成一个可跑的用例",
                        "先测基线再动手:无基线不优化,删减优先",
                        "新用户漏斗走查:亲手走一遍,只记录真实摩擦",
                        "破坏性演练:拿坏输入砸,坏行为当场修",
                    ];

                    // 同理的**正文**台账,而且没有它 Pass 1 就是死代码:
                    // plan/16/17 把这五条的正文整个重写了(裸 `###` 提示词 →
                    // 规范 SKILL.md body),于是「pristine = content 与今日
                    // canon 逐字节相等」对它要迁移的那批老行永远为假 ——
                    // 老行既升不了源、也就永远轮不到 Pass 2 对账,会一直顶
                    // 着旧正文停在 SelfBuilt,还被 SkillHub 点上「规范 ·
                    // 待校正」黄徽记。这里逐字收录改写前的正本正文(与
                    // LEGACY_CANON_DESCS 同一性质:一份有界的历史台账,不是
                    // 版本跑步机——它是固定的历史,不随以后每次改写增长)。
                    const LEGACY_CANON_CONTENTS: [&str; 5] = [
                        "### 证据先行 (evidence-first)\n\
                 1. 只记录两类内容:(a) 你直接验证过的事实(真实命令输出、真实文件内容);\
                 (b) 你的先验知识——必须标注「未核实」。\n\
                 2. 每条证据注明来源:文件路径、命令、或「知识截止内记忆,未核实」。\n\
                 3. 禁止编造统计数字与引用;没有可靠数字就写「无可靠数字」。\n\
                 4. 结论按「证据 → 洞察 → 假设」链书写,断链处如实标断。",
                        "### 规格即测试 (spec-to-tests)\n\
                 1. SPEC 里每条验收标准编号(AC-1, AC-2, …);写实现前先把它翻译成测试名\
                 (如 `ac1_reports_dead_relative_link`)。\n\
                 2. 无法翻译成测试的验收标准是坏标准——回头改写它,而不是跳过。\n\
                 3. 实现只做到让测试通过为止,不做规格外功能。\n\
                 4. 提交前 `cargo test` 全绿是硬门禁;失败输出原样留档,不美化。",
                        "### 先测基线再动手 (baseline-before-touch)\n\
                 1. 动手前先真实测量并落盘:测试数、clippy 警告数、代码行数、构建耗时——\
                 全部来自真实命令输出的原样摘录。\n\
                 2. 每步重构保持测试全绿;一步只做一类等价变换。\n\
                 3. 删减优先:能删的代码是最好的优化,删除行数计入成果。\n\
                 4. 结束时用与基线完全相同的命令重测,报 delta;无 delta 也如实报。",
                        "### 新用户漏斗走查 (fresh-eyes-funnel)\n\
                 1. 以从未见过本项目的人的视角,真实执行「发现 → 安装 → 首次使用 → 再次使用」\
                 每一步,不跳步、不脑补。\n\
                 2. 只记录你真实遇到的摩擦(命令报错、文档缺失、参数不明),不臆想用户。\n\
                 3. 一次实验只改一个变量,改动前后用同一条真实命令对照。\n\
                 4. 没有真实流量就如实做「前后对照」,不假装有 A/B 分流。",
                        "### 破坏性演练 (breaking-drill)\n\
                 1. 系统性地喂坏输入:不存在的路径、空输入、超长输入、坏参数、坏编码——\
                 逐个真实执行并原样记录行为。\n\
                 2. 任何 panic 或不知所云的报错都算事故:当场修复成友好报错,修后重测。\n\
                 3. 健康检查脚本必须一键可跑、任何失败以非零码退出;写完真实执行一遍留档。\n\
                 4. 复盘只引用真实存在的文件与提交号(写之前 ls / git log 核实)。",
                    ];

                    // Pass 1 · pristine promotion (plan/16 §4): a stage-skill
                    // row seeded by a pre-T2/pre-plan/16 binary reads back
                    // `SelfBuilt` (legacy `official`+空库名 encoding, or the
                    // T2-era `SelfBuilt` seed). Pristine = `content` equals
                    // the code canon byte-for-byte AND `desc` is a canonical
                    // text (today's or the fixed pre-plan/16 one) — then
                    // nobody made it their own, and it gets the label fresh
                    // seeds now carry: `Official { "bw-standard" }`, which is
                    // what licenses Pass 2 to reconcile it. Anything else
                    // stays `SelfBuilt`, honestly: that's a user edit, never
                    // to be washed away by Boot. Two hard exclusions:
                    // distilled rows (provenance beats relabelling), and
                    // rows carrying `adapted_from` — that trace means a
                    // human already detached this row from an official
                    // origin via T11, so re-promoting it would wash a
                    // desc/category-only edit right back out (the flip loop
                    // /code-review caught: edit desc → T11 flips SelfBuilt
                    // with content untouched → naive Pass 1 re-promotes →
                    // Pass 2 clobbers the desc).
                    let existing_skills = self.store.list_skills().await?;
                    for c in &skill_canon {
                        for row in existing_skills.iter().filter(|s| {
                            s.name == c.name
                                && s.source == HubSource::SelfBuilt
                                && s.adapted_from.is_none()
                                && s.distilled_from_issue.is_none()
                                && (s.content == c.content
                                    || LEGACY_CANON_CONTENTS.contains(&s.content.as_str()))
                                && (s.desc == c.desc
                                    || LEGACY_CANON_DESCS.contains(&s.desc.as_str()))
                        }) {
                            self.store
                                .set_skill_source(
                                    row.id,
                                    HubSource::Official {
                                        official_library: BW_STANDARD_LIBRARY.to_string(),
                                    },
                                )
                                .await?;
                        }
                    }

                    // Pass 2 · reconcile. Only ever touches the real
                    // bw-standard row — a user's own self-built or distilled
                    // skill that happens to share the name is never a
                    // candidate. Deliberately NOT `Command::UpdateSkill`/
                    // `self.store.update_skill` with `flip_to_self_built:
                    // true`: that flip rule (T11, "编辑即脱离源头") exists
                    // for a *human* diverging a skill from its official
                    // origin — this is the opposite motion, the official
                    // origin catching a row back up to itself, so `source`
                    // must stay `Official { "bw-standard" }` unchanged.
                    // `filter` 而非 `find`:与 Pass 1 同口径。唯一性守卫
                    // (S2)是本轮才加的,只管新写入——存量库里若有两行同名
                    // 且都被 Pass 1 升成 Official{bw-standard},`find` 只会
                    // 追平其中一行,另一行顶着官方徽记停在旧内容,而 UI 分
                    // 不出二者。
                    let existing_skills = self.store.list_skills().await?;
                    for c in &skill_canon {
                        for existing in existing_skills.iter().filter(|s| {
                            s.name == c.name
                                && matches!(
                                    &s.source,
                                    HubSource::Official { official_library }
                                        if official_library == BW_STANDARD_LIBRARY
                                )
                        }) {
                            if existing.content == c.content && existing.desc == c.desc {
                                continue;
                            }
                            // `uses` / `distilled_from_issue` / `origin_agent`
                            // are derived lifecycle fields (skill-standards:
                            // "永不手填") — `SkillEdit` has no fields for them at
                            // all, so there is no way this call could touch
                            // them even by accident.
                            self.store
                                .update_skill(
                                    existing.id,
                                    SkillEdit {
                                        name: existing.name.clone(),
                                        desc: c.desc.clone(),
                                        category: existing.category.clone(),
                                        content: c.content.clone(),
                                        flip_to_self_built: false,
                                    },
                                )
                                .await?;
                        }
                    }
                }
                // plan/19 §6 P0 + 2026-08-05 拍板「直接作为基础 skill 引入」:
                // 盲测冠军 mohit `metrics-framework`/`metric-tree-builder`
                // (引领/滞后组两轮 8 票均值 9.00 第一,MIT,vendored 于
                // examples/skill-libraries/mohit-pm-claude-skills)从「手跑
                // example 才入库」升级为 Boot 自动引入——含全新 DB 在内的每
                // 个库都自带这两件基础技能。走既有 `ImportSkillLibrary` 命令
                // 而非 bw-standard canon 播种,因为:①出处保真——行的
                // `official_library` 就是真实上游
                // `mohitagw15856/pm-claude-skills`,不伪装成 bw-standard
                // (plan/16 S6 来源诚实 + plan/19「原文保留」拍板);②包内
                // references/templates 支撑文件随包一并入库(canon 播种路径
                // 只存正文);③幂等与 T11「编辑即脱离源头」复用该命令自己的
                // `(name, official_library)` + `adapted_from` 判定,不另造第
                // 二套。vendor 目录缺失(脱离仓库运行的打包场景)按 ECC
                // vendor 先例(`legacy_migration::ecc_agents_vendor_dir`)
                // 如实跳过——绝不硬造,也不报错拦启动。
                const MOHIT_VENDOR_ROOT: &str = concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../examples/skill-libraries/mohit-pm-claude-skills/skills"
                );
                if matches!(
                    skill_import::find_skill_package_dirs(MOHIT_VENDOR_ROOT),
                    Ok(dirs) if !dirs.is_empty()
                ) {
                    Box::pin(self.dispatch(Command::ImportSkillLibrary {
                        root_path: MOHIT_VENDOR_ROOT.to_string(),
                        official_library: "mohitagw15856/pm-claude-skills".to_string(),
                        project_id: None,
                    }))
                    .await?;
                }
                // A4: backfill the per-stage "完成 Issue 数" metric for every
                // project — pre-A4 projects gain it; already-seeded ones are
                // unchanged (by-name idempotent).
                for p in &projects {
                    self.seed_stage_done_metrics(p.id).await?;
                    self.seed_codehub_public_metrics(p.id).await?;
                    // plan/20 W1 (R1): 存量项目补种自有五角色副本——按
                    // (project, name) 幂等,重启不重复;全局五行(共享目录
                    // 模板)原地不动,战绩不迁移。
                    bw_store::seed_project_role_agents_if_missing(self.store.as_ref(), p.id)
                        .await?;
                }
                self.refresh_workflow_specs().await?;
                // 五角色归类对账(SR5):放在 mohit `ImportSkillLibrary` 之后
                // ——那次导入会新建两行技能,对账要看得见它们;放在
                // `refresh_skills` 之前,让本次 Boot 里的界面立刻读到对账后
                // 的 stages。
                self.reconcile_skill_stages().await?;
                self.refresh_skills().await?;
                self.refresh_agents().await?;
                self.refresh_cron_tasks().await?;
                self.refresh_connectors().await?;
                self.refresh_knowledge_sources().await?;
                self.refresh_activity().await?;
                self.refresh_issues().await?;
                self.emit(Event::ProjectsChanged);
                // vendored 包文档坏了(开发者错误)的诚实归宿:整个 Boot 的
                // 其余部分已经跑完、状态不半初始化,错误在这里原样抛出,由
                // 桌面壳呈现。
                if let Some(e) = canon_err {
                    return Err(AppError::Invalid(e));
                }
            }

            Command::CreateProject {
                provider,
                id,
                name,
                kind,
                desc,
                workspace,
                github,
                codehub,
            } => {
                self.store
                    .create_project(NewProject {
                        id,
                        name,
                        kind,
                        desc,
                        provider,
                    })
                    .await?;
                self.state.active_project = Some(id);
                self.state.view = View::Create;
                // plan/20 W1 (R1): 出生即带自有五角色队友(从代码正本复制,
                // 战绩从零立账)——「新项目里指派下拉只出现自己的五个角色」
                // (plan/08 S1 完成标准)从出生那一刻成立。
                bw_store::seed_project_role_agents_if_missing(self.store.as_ref(), id).await?;
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
                // P1: 建项目即建仓 —— 出生那一刻仓就存在(而非走完创建流才有)。
                // 绑定已有本地仓:只校验含 .git,绝不动原文件。GitHub 为主体
                // (2026-07-22): github 非空时改走 gh CLI 开仓/接入,新建失败
                // 软降级回本地 mint,接入失败不兜底(不拿无关空仓冒充)。两条
                // 路径都绝不让 CreateProject 本身失败——只有本地 bind 校验例外。
                let bound = workspace
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                match (bound, github, codehub) {
                    (Some(path), _, _) => {
                        if !std::path::Path::new(path).join(".git").exists() {
                            return Err(AppError::Invalid(format!(
                                "绑定的工作目录不是 git 仓库(无 .git):{path}"
                            )));
                        }
                        self.store.set_workspace(id, path, true).await?;
                    }
                    (None, Some(GithubOrigin::New { slug, private }), _) => {
                        // plan/14 C14: 建仓是这个命令里最慢的一步(真实
                        // `gh repo create` 网络调用,数秒)——Started 先发,
                        // 唯一的 name 贯穿这一步的 Ok/Fail,UI 据此配对。
                        let action_name = format!("{} · 建仓", proj.name);
                        self.emit(Event::ActionProgress {
                            name: action_name.clone(),
                            state: ActionState::Started,
                        });
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let body = if proj.desc.trim().is_empty() {
                                    "(创建流程未填写 brief)".to_string()
                                } else {
                                    proj.desc.trim().to_string()
                                };
                                match bw_engine::github::create_repo(
                                    &slug, private, &root, &proj.name, &body,
                                )
                                .await
                                {
                                    Ok(r) => {
                                        let path = root.join(&slug).to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &path, true).await?;
                                        self.store
                                            .set_remote(
                                                id,
                                                "github.com",
                                                &format!("{}/{}", r.owner, r.repo),
                                            )
                                            .await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · GitHub", proj.name),
                                                kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.owner, r.repo),
                                            })
                                            .await?;
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Ok(format!(
                                                "{}/{}",
                                                r.owner, r.repo
                                            )),
                                        });
                                    }
                                    Err(e) => {
                                        // plan18-⑧:建仓失败就停、如实报,不兜底
                                        // 本地 mint 空仓装接上(缺口 E:悄悄本地 mint
                                        // → remote_path 空 → 不建 trio/不挂 cron/不扫
                                        // skill,用户以为建好了实际远端没仓)。项目行
                                        // 已落库但无远端仓,用户看到 Fail 可删项目重
                                        // 来或重试——和"不假装健康"同精神。
                                        let detail = format!(
                                            "GitHub 建仓失败:{e}(未接上,无远端仓,请重试或检查权限)"
                                        );
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · GitHub", proj.name),
                                            ok: false,
                                            detail: detail.clone(),
                                        });
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Fail(detail),
                                        });
                                    }
                                }
                            }
                            None => {
                                self.fail_no_workspaces_root(
                                    &action_name,
                                    &proj.name,
                                    "GitHub",
                                    "未配置本地工作区根目录,无法建仓",
                                );
                            }
                        }
                    }
                    (None, Some(GithubOrigin::Existing { owner, repo }), _) => {
                        // plan/14 C14: 克隆同样是真实 `gh repo clone` 网络
                        // 调用——同一套 Started→Ok/Fail 配对。
                        let action_name = format!("{} · 克隆仓库", proj.name);
                        self.emit(Event::ActionProgress {
                            name: action_name.clone(),
                            state: ActionState::Started,
                        });
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let dir = root.join(workspace_slug(&proj.name, id));
                                match bw_engine::github::clone_repo(&owner, &repo, &dir).await {
                                    Ok(r) => {
                                        let path = dir.to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &path, true).await?;
                                        self.store
                                            .set_remote(
                                                id,
                                                "github.com",
                                                &format!("{}/{}", r.owner, r.repo),
                                            )
                                            .await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · GitHub", proj.name),
                                                kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.owner, r.repo),
                                            })
                                            .await?;
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Ok(format!(
                                                "{}/{}",
                                                r.owner, r.repo
                                            )),
                                        });
                                        // V2-② Phase A (§6.2): later-comer
                                        // detection — if the cloned repo already
                                        // has `.bw/project.toml`, this Buddy is
                                        // a later-comer. The actual sync (reading
                                        // back the canonical intent + metrics +
                                        // connectors) happens in CompleteCreation
                                        // (after UpdateBrief has run, so the
                                        // synced values aren't overwritten by
                                        // the Intent card's local input). This
                                        // signal just informs the user.
                                        if has_project_toml(&path) {
                                            self.emit(Event::ConnectorSynced {
                                                name: format!("{} · project.toml", proj.name),
                                                ok: true,
                                                detail: "仓里已有 .bw/project.toml(后来者接入),意图正本将在完成创建时读回".into(),
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        // 不兜底本地 mint —— 拿一个跟用户选的仓无关
                                        // 的空仓冒充"已接入",比"暂不挂仓库"更不诚实。
                                        let detail = format!("接入 {owner}/{repo} 失败:{e}");
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · GitHub", proj.name),
                                            ok: false,
                                            detail: detail.clone(),
                                        });
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Fail(detail),
                                        });
                                    }
                                }
                            }
                            None => {
                                self.fail_no_workspaces_root(
                                    &action_name,
                                    &proj.name,
                                    "GitHub",
                                    "未配置本地工作区根目录,无法接入",
                                );
                            }
                        }
                    }
                    (None, None, Some(CodehubOrigin::Existing { host, path })) => {
                        // codehub 接入已有仓(对标 github Existing):真实
                        // `codehub-cli repo clone` 网络调用,同一套 Started→Ok/Fail。
                        let action_name = format!("{} · 克隆 codehub 仓", proj.name);
                        self.emit(Event::ActionProgress {
                            name: action_name.clone(),
                            state: ActionState::Started,
                        });
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let dir = root.join(workspace_slug(&proj.name, id));
                                match bw_engine::codehub::clone_repo(&host, &path, &dir).await {
                                    Ok(()) => {
                                        let p = dir.to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &p, true).await?;
                                        self.store.set_remote(id, &host, &path).await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · CodeHub", proj.name),
                                                kind: CONNECTOR_KIND_CODEHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{host}/{path}"),
                                            })
                                            .await?;
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Ok(path.clone()),
                                        });
                                        // V2-② Phase A (§6.2): later-comer
                                        // detection (same as github Existing).
                                        if has_project_toml(&p) {
                                            self.emit(Event::ConnectorSynced {
                                                name: format!("{} · project.toml", proj.name),
                                                ok: true,
                                                detail: "仓里已有 .bw/project.toml(后来者接入),意图正本将在完成创建时读回".into(),
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        let detail = format!("接入 codehub {host}/{path} 失败:{e}");
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · CodeHub", proj.name),
                                            ok: false,
                                            detail: detail.clone(),
                                        });
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Fail(detail),
                                        });
                                    }
                                }
                            }
                            None => {
                                self.fail_no_workspaces_root(
                                    &action_name,
                                    &proj.name,
                                    "CodeHub",
                                    "未配置本地工作区根目录,无法克隆",
                                );
                            }
                        }
                    }
                    (
                        None,
                        None,
                        Some(CodehubOrigin::New {
                            host,
                            namespace,
                            name: repo_name,
                            visibility,
                        }),
                    ) => {
                        // codehub 新建仓(对标 github New):真实
                        // `codehub-cli project create` + `git clone` + BW root
                        // commit(让 is_owned_workspace=true)。同一套 Started→Ok/Fail。
                        let action_name = format!("{} · 建 codehub 仓", proj.name);
                        self.emit(Event::ActionProgress {
                            name: action_name.clone(),
                            state: ActionState::Started,
                        });
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let dir = root.join(workspace_slug(&proj.name, id));
                                let body = if proj.desc.trim().is_empty() {
                                    "(创建流程未填写 brief)".to_string()
                                } else {
                                    proj.desc.trim().to_string()
                                };
                                match bw_engine::codehub::create_repo(
                                    &host,
                                    &namespace,
                                    &repo_name,
                                    &visibility,
                                    &dir,
                                    &proj.name,
                                    &body,
                                )
                                .await
                                {
                                    Ok(r) => {
                                        let p = dir.to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &p, true).await?;
                                        self.store.set_remote(id, &r.host, &r.path).await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · CodeHub", proj.name),
                                                kind: CONNECTOR_KIND_CODEHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.host, r.path),
                                            })
                                            .await?;
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Ok(r.path.clone()),
                                        });
                                    }
                                    Err(e) => {
                                        let detail = format!(
                                            "codehub 建仓失败:{e}(未接上,无远端仓,请重试或检查权限)"
                                        );
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · CodeHub", proj.name),
                                            ok: false,
                                            detail: detail.clone(),
                                        });
                                        self.emit(Event::ActionProgress {
                                            name: action_name,
                                            state: ActionState::Fail(detail),
                                        });
                                    }
                                }
                            }
                            None => {
                                self.fail_no_workspaces_root(
                                    &action_name,
                                    &proj.name,
                                    "CodeHub",
                                    "未配置本地工作区根目录,无法建仓",
                                );
                            }
                        }
                    }
                    (None, None, None) => {
                        if let Some(root) = self.workspaces_root.clone() {
                            match provision_workspace(&root, &proj).await {
                                Ok(path) => {
                                    self.store.set_workspace(id, &path, true).await?;
                                }
                                Err(e) => {
                                    self.emit(Event::ConnectorSynced {
                                        name: format!("{} · 代码仓", proj.name),
                                        ok: false,
                                        detail: format!("自动开仓失败,项目将以 Mock 模式运行:{e}"),
                                    });
                                }
                            }
                        }
                    }
                }
                // V1 Issue 1 phase2 · 工作区探活(不建 git-repo connector,直接
                // 调一次):evidence::collect + feed_workspace_metrics +
                // sync_project_assets 原在 probe_connector 的 git-repo arm,现搬
                // 到创建时直接调,采第一批工作区指标 + 扫 assets。
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                if !proj.workspace_path.trim().is_empty() {
                    self.probe_workspace(id, &proj.workspace_path, "CreateProject")
                        .await;
                    // 建 script connector(挂远端的项目;buddy 自带采集脚本,
                    // §0 第 2 层业务脚本)。脚本写进 .bw/collect_stats.sh(相对
                    // 工作区,buddy 自有空间),collect arm 跑脚本 → 读输出 JSON。
                    if !proj.remote_path.trim().is_empty() {
                        write_buddy_collect_stats(&proj);
                        let config = serde_json::json!({
                            "script": ".bw/collect_stats.sh",
                            "output": ".bw/collect_stats.json",
                            "command": "sh",
                        })
                        .to_string();
                        self.store
                            .create_connector(NewConnector {
                                id: ConnectorId::new(),
                                name: format!("{} · 仓统计", proj.name),
                                kind: CONNECTOR_KIND_SCRIPT.into(),
                                scope: proj.name.clone(),
                                project_id: Some(id),
                                config,
                            })
                            .await?;
                    }
                }
                // C7 · 标配采集 cron (plan/13 D7):挂了远端仓的项目出生即
                // 带一条每日采集器,由现成 tick_scheduler 到点真实触发,把
                // 远端数据拉成 append-only 观测。挂远端的项目(github/codehub
                // 均算)挂;软降级回本地/接入失败的项目 remote_path 仍空,不挂
                // ——不给采不到的东西装一个空跑的 cron。no-hijack:
                // CollectMetrics 只观测,绝不自动跑活/结算。
                let github_backed = self
                    .store
                    .get_project(id)
                    .await?
                    .map(|pr| !pr.remote_path.trim().is_empty())
                    .unwrap_or(false);
                if github_backed {
                    self.store
                        .create_cron_task(NewCronTask {
                            id: CronTaskId::new(),
                            name: format!("{} · 采集代码仓指标", proj.name),
                            target: String::new(),
                            schedule: Cadence::Daily,
                            project_id: Some(id),
                            mode: CronMode::CollectMetrics,
                            issue_stage: None,
                            issue_assignee: None,
                            // PF1-4: 填 now() 防新建 cron 在 clone/setup 完成
                            // 前抢跑(cron_due 首 tick 立即触发 → workspace_path
                            // 仍空 → 脚本臂跳过 → 记 normal → 下次 tick 等明天)。
                            last_run_at: Some(now().unix_timestamp()),
                        })
                        .await?;
                    self.refresh_cron_tasks().await?;
                    self.emit(Event::CronTasksChanged);
                }
                if let Err(e) = write_charter(self, id, "开篇").await {
                    self.emit(Event::ActionProgress {
                        name: "写项目章程".into(),
                        state: ActionState::Fail(format!(
                            "章程未写入仓（可能缺 PROJECT.md，请人工补）：{e}"
                        )),
                    });
                }
                // 模板能力(用户 2026-07-20 拍板):四份组件标准文件写进仓里,
                // 供人与 agent 之后在这个项目里创建 agent/skill/workflow/cron 时
                // 对照(同一 owned-workspace 门槛,一次性,不随创建流逐步改写)。
                if let Err(e) = write_component_standards(self, id).await {
                    self.emit(Event::ActionProgress {
                        name: "写组件标准".into(),
                        state: ActionState::Fail(format!(
                            "组件标准未写入仓（.claude/standards/ 可能缺，请人工补）：{e}"
                        )),
                    });
                }
                self.refresh_projects().await?;
                self.refresh_connectors().await?;
                self.emit(Event::ProjectsChanged);
                self.emit(Event::ViewChanged(View::Create));
            }

            Command::ListGithubRepos => {
                // plan/14 C14: the Repo 卡片's「接入已有仓」picker triggers a
                // real `gh repo list` call — same Started→Ok/Fail pairing.
                const ACTION_NAME: &str = "GitHub 仓库列表";
                self.emit(Event::ActionProgress {
                    name: ACTION_NAME.into(),
                    state: ActionState::Started,
                });
                match bw_engine::github::list_repos(30).await {
                    Ok(repos) => {
                        self.emit(Event::ActionProgress {
                            name: ACTION_NAME.into(),
                            state: ActionState::Ok(format!("{} 个仓库", repos.len())),
                        });
                        self.state.github_repos = repos;
                    }
                    Err(e) => {
                        self.state.github_repos = Vec::new();
                        self.emit(Event::ConnectorSynced {
                            name: ACTION_NAME.into(),
                            ok: false,
                            detail: e.to_string(),
                        });
                        self.emit(Event::ActionProgress {
                            name: ACTION_NAME.into(),
                            state: ActionState::Fail(e.to_string()),
                        });
                    }
                }
                self.emit(Event::ProjectsChanged);
            }

            Command::ListCodehubRepos { host } => {
                // V1 Issue 1: the Repo 卡片's「接入已有仓」picker for codehub
                // triggers a real `codehub-cli project list --mine` call —
                // same Started→Ok/Fail pairing as `ListGithubRepos`. `host`
                // = green/open/yellow(codehub 三域名,需显式带)。
                const ACTION_NAME: &str = "CodeHub 仓库列表";
                self.emit(Event::ActionProgress {
                    name: ACTION_NAME.into(),
                    state: ActionState::Started,
                });
                match bw_engine::codehub::list_repos(&host, 30).await {
                    Ok(repos) => {
                        self.emit(Event::ActionProgress {
                            name: ACTION_NAME.into(),
                            state: ActionState::Ok(format!("{} 个仓库", repos.len())),
                        });
                        self.state.codehub_repos = repos;
                    }
                    Err(e) => {
                        self.state.codehub_repos = Vec::new();
                        // PF1-5: credentials/token/secret 类错因映射人话,告诉
                        // 用户去本机 codehub-cli auth login(决议 6:不硬编码禁点
                        // yellow,失败映射人话 + 保留现有警告 + toast 自清)。
                        let raw = e.to_string();
                        let lower = raw.to_lowercase();
                        let detail = if lower.contains("credential")
                            || lower.contains("token")
                            || lower.contains("secret")
                            || lower.contains("auth")
                            || lower.contains("login")
                            || lower.contains("401")
                        {
                            format!(
                                "{host} 域未登录:先本机 `codehub-cli -H {host} auth login`(原始错因:{raw})"
                            )
                        } else {
                            raw
                        };
                        self.emit(Event::ConnectorSynced {
                            name: ACTION_NAME.into(),
                            ok: false,
                            detail,
                        });
                        self.emit(Event::ActionProgress {
                            name: ACTION_NAME.into(),
                            state: ActionState::Fail(e.to_string()),
                        });
                    }
                }
                self.emit(Event::ProjectsChanged);
            }

            Command::ClearRemoteProjectProbe => {
                self.state.remote_project_probe = RemoteProjectProbe::Idle;
                self.emit(Event::ProjectsChanged);
            }

            Command::ProbeRemoteProjectToml {
                provider,
                host,
                path,
                default_branch,
            } => {
                // V2-② Intent UX (§6.2): pre-clone remote probe. Quiet on the
                // action strip (Intent card shows Probing itself); never
                // invent later-comer on error.
                let path = path.trim().to_string();
                if path.is_empty() {
                    self.state.remote_project_probe = RemoteProjectProbe::Idle;
                    self.emit(Event::ProjectsChanged);
                } else {
                    self.state.remote_project_probe = RemoteProjectProbe::Probing;
                    self.emit(Event::ProjectsChanged);
                    let result = if provider == "codehub" {
                        bw_engine::codehub::fetch_project_toml(&host, &path, &default_branch)
                            .await
                            .map_err(|e| e.to_string())
                    } else {
                        // github: path = owner/repo
                        let mut parts = path.splitn(2, '/');
                        let owner = parts.next().unwrap_or("").to_string();
                        let repo = parts.next().unwrap_or("").to_string();
                        if owner.is_empty() || repo.is_empty() {
                            Err("GitHub 仓路径应为 owner/repo".into())
                        } else {
                            bw_engine::github::fetch_project_toml(&owner, &repo, &default_branch)
                                .await
                                .map_err(|e| e.to_string())
                        }
                    };
                    self.state.remote_project_probe = match result {
                        Ok(Some(file)) => RemoteProjectProbe::Present(file),
                        Ok(None) => RemoteProjectProbe::Absent,
                        Err(e) => RemoteProjectProbe::Failed(e),
                    };
                    self.emit(Event::ProjectsChanged);
                }
            }

            Command::SetCycle { cycle } => {
                let p = self.active()?;
                self.store.set_project_cycle(p, cycle).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateBrief {
                benchmark,
                opportunity,
            } => {
                let p = self.active()?;
                self.store.set_brief(p, &benchmark, &opportunity).await?;
                if let Err(e) = write_charter(self, p, "定位与机会").await {
                    self.emit(Event::ActionProgress {
                        name: "写项目章程".into(),
                        state: ActionState::Fail(format!(
                            "章程未补写（PROJECT.md 定位与机会段可能缺）：{e}"
                        )),
                    });
                }
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateNorthStar { value, def } => {
                let p = self.active()?;
                self.store.set_north_star(p, &value, &def).await?;
                if let Err(e) = write_charter(self, p, "北极星").await {
                    self.emit(Event::ActionProgress {
                        name: "写项目章程".into(),
                        state: ActionState::Fail(format!(
                            "章程未补写（PROJECT.md 北极星段可能缺）：{e}"
                        )),
                    });
                }
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateProjectIdentity { name, kind, descr } => {
                let p = self.active()?;
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .set_project_identity(p, &name, kind.trim(), descr.trim())
                    .await?;
                if let Err(e) = write_charter(self, p, "项目信息").await {
                    self.emit(Event::ActionProgress {
                        name: "写项目章程".into(),
                        state: ActionState::Fail(format!(
                            "章程未补写（PROJECT.md 项目信息段可能缺）：{e}"
                        )),
                    });
                }
                self.refresh_projects().await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpsertManualMetric {
                id,
                name,
                def,
                role,
                stage_kind,
                target,
                amber,
                value,
            } => {
                let p = self.active()?;
                // Idempotency guard: re-confirming a step must not mint a
                // duplicate observation — only a *changed* value is a new fact.
                let latest = self
                    .store
                    .persisted_signals(p)
                    .await?
                    .metrics
                    .into_iter()
                    .find(|m| m.id == id)
                    .map(|m| m.value_raw);
                self.store
                    .upsert_metric(NewMetric {
                        id,
                        project_id: p,
                        role,
                        stage_kind,
                        name,
                        def,
                        target_raw: target,
                        amber,
                        last_target: String::new(),
                        driver: String::new(),
                        pos: 0,
                        collect_kind: "manual".into(),
                        collect_query: String::new(),
                    })
                    .await?;
                // The value is born as an explicit Manual observation; the signal
                // it implies is computed later by recompute, never set here.
                let value = value.trim();
                if !value.is_empty() && latest.as_deref() != Some(value) {
                    self.store
                        .append_observation(id, SourceKind::Manual, value, now())
                        .await?;
                }
                self.emit(Event::ProjectUpdated(p));
            }

            Command::SetMetricArchived { metric, archived } => {
                let p = self.active()?;
                // 作用域守卫:只能停用/恢复当前项目自己的指标。指标跟着项目
                // 走,没有跨项目的停用 —— 一个别的项目的 MetricId 传进来直接
                // NotFound,而不是静默改到别人头上。
                let sigs = self.store.persisted_signals(p).await?;
                let target = sigs
                    .metrics
                    .iter()
                    .find(|m| m.id == metric)
                    .ok_or(AppError::NotFound)?;
                if target.archived == archived {
                    // 幂等:重复点不重复盖 archived_at 时戳,也不白重算一遍。
                    return Ok(());
                }
                self.store.set_metric_archived(metric, archived).await?;
                // 这条指标进/出了上卷集合 ⇒ 项目与阶段的健康灯要重算。
                // 唯一写入者仍是 recompute_signals,绝不手工 patch。
                self.store.recompute_signals(p, now()).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::RecordObservation { metric, value } => {
                let p = self.active()?;
                let value = value.trim();
                if value.is_empty() {
                    return Err(AppError::Invalid("观测值不能为空".into()));
                }
                self.store
                    .append_observation(metric, SourceKind::Manual, value, now())
                    .await?;
                self.store.recompute_signals(p, now()).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::RecordCollectedObservation {
                metric,
                value,
                source,
            } => {
                let p = self.active()?;
                let value = value.trim();
                if value.is_empty() {
                    return Err(AppError::Invalid("观测值不能为空".into()));
                }
                if matches!(source, SourceKind::Manual) {
                    // A hand-typed value must go through `RecordObservation`
                    // and wear its `手填` badge — letting a caller stamp
                    // `Manual` here would blur the one line this command
                    // exists to draw (machine-measured vs hand-entered).
                    return Err(AppError::Invalid(
                        "机器采集观测不能标记为 Manual——请走 RecordObservation".into(),
                    ));
                }
                self.store
                    .append_observation(metric, source, value, now())
                    .await?;
                self.store.recompute_signals(p, now()).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::SetStageProgress {
                stage_kind,
                progress,
            } => {
                let p = self.active()?;
                self.store
                    .set_stage_progress(p, stage_kind, progress)
                    .await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::ToggleDod { stage_kind, index } => {
                let p = self.active()?;
                self.store.toggle_dod(p, stage_kind, index).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::HandoffStage { risky, note } => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                let from = proj.active_stage;
                let to = from.next();
                // A4: leaving a stage with unfinished (non-terminal) issues is a
                // risky handoff by definition — force it honest + tag the note,
                // so open work can't slip silently into the next stage.
                let open_in_stage = self
                    .store
                    .list_issues(p, Some(from), None)
                    .await?
                    .iter()
                    .filter(|i| !i.status.is_terminal())
                    .count();
                let (risky, note) = if open_in_stage > 0 {
                    let tag = format!("留 {} 件未完 Issue;", open_in_stage);
                    let note = if note.trim().is_empty() {
                        tag
                    } else {
                        format!("{tag} {note}")
                    };
                    (true, note)
                } else {
                    (risky, note)
                };
                self.store
                    .handoff_stage(p, from, to, risky, &note, now())
                    .await?;
                self.refresh_projects().await?;
                self.refresh_activity().await?;
                self.emit(Event::StageHandoff { from, to, risky });
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ActivityChanged);
            }

            Command::CompleteCreation { cadence, run_first } => {
                let p = self.active()?;
                self.store.set_project_phase(p, Readiness::Running).await?;
                self.store
                    .materialize_stages(five_stages(p, cadence))
                    .await?;
                // A4: seed the per-stage "完成 Issue 数" leading metric (empty
                // target ⇒ honest Unknown) so Done-edge feeds have a home. The
                // recompute at the end of CompleteCreation derives its signal.
                self.seed_stage_done_metrics(p).await?;
                // P5:codehub 公共指标(开放 Issue 数 / 已合入 MR 数)随创建流
                // 种下,走 collect_project_metrics 的 codehub arm 真采点亮。
                self.seed_codehub_public_metrics(p).await?;
                // All-in-one-codebase default: a project completing creation
                // gets its own real git repo (when a workspaces root is
                // configured and no workspace was set by hand), plus a bound
                // `git-repo` connector. Provisioning failure degrades to the
                // old Mock-only behavior — creation itself never breaks.
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                // plan18-⑧:只对纯本地项目(remote_path 也空)兜底开本地仓;
                // 挂了远端但 clone 失败导致 workspace_path 空的,不兜底本地
                // mint 装接上(缺口 C:否则用户以为接了远端实际是个本地空仓,
                // remote_path 非空还会建 trio/cron 但无 workspace 跑不了)。
                // 挂远端失败的项目停在"有 remote 无 workspace"——用户看到无
                // 代码仓 connector 知道没接上,可删项目重来,比悄悄 mint 诚实。
                if self.workspaces_root.is_some()
                    && proj.workspace_path.trim().is_empty()
                    && proj.remote_path.trim().is_empty()
                {
                    let root = self.workspaces_root.clone().expect("checked above");
                    match provision_workspace(&root, &proj).await {
                        Ok(path) => {
                            self.store.set_workspace(p, &path, true).await?;
                            // V1 Issue 1 phase2: 不建 git-repo connector,
                            // 直接调一次工作区探活(采第一批指标 + 扫 assets)。
                            self.probe_workspace(p, &path, "CompleteCreation").await;
                            self.refresh_connectors().await?;
                            self.emit(Event::ConnectorsChanged);
                        }
                        Err(e) => {
                            // Loud, honest degradation — never a silent fake.
                            self.emit(Event::ConnectorSynced {
                                name: format!("{} · 代码仓", proj.name),
                                ok: false,
                                detail: format!("自动开仓失败,项目将以 Mock 模式运行:{e}"),
                            });
                        }
                    }
                }
                // C8 · plan/13 D8: 挂仓项目(remote_path 非空)创建流落地
                // 即建标配 Issue 三件套(竞品分析→找指标→绑数据),依赖序即
                // 建单序即编号序(1/2/3),都经既有 sync_issue_to_github 真开
                // GitHub issue。无仓项目(remote_path 空——包括新建/接入
                // 都失败、软降级回本地 mint 的项目)零标配票:不给建不了
                // 仓、没有 PR 环可走的项目发一套没处交付的活,如实留白。
                let first_issue = self.seed_standard_issue_trio(p).await?;
                // V2-②-I: after trio (first-comer) or skip (later-comer), pull
                // remote open issues into local Backlog rows. Idempotent on
                // github_number — first-comer refreshes the trio it just
                // minted; later-comer rebuilds so the board is runnable.
                // Soft-fail: sync errors toast, never abort creation.
                if !proj.remote_path.trim().is_empty() {
                    let _ = self.sync_remote_issues_for(p, true).await;
                }
                // V2-② Phase A (§6.1#4 / §6.2): both first-comer and later-comer
                // read back metrics/connectors from the repo when a real
                // workspace exists (mature repo may already have them before
                // any project.toml). Later-comer also syncs project.toml so
                // Intent-card local input is overridden by the repo正本.
                // SyncProjectFile is a no-op when the file is absent.
                if !proj.workspace_path.trim().is_empty() {
                    self.sync_project_file_for(p).await?;
                    self.sync_metrics_file_for(p).await?;
                    self.sync_connectors_file_for(p).await?;
                }
                self.store.recompute_signals(p, now()).await?;
                if let Err(e) = write_charter(self, p, "完成创建").await {
                    self.emit(Event::ActionProgress {
                        name: "写项目章程".into(),
                        state: ActionState::Fail(format!(
                            "章程未补写（PROJECT.md 完成创建段可能缺）：{e}"
                        )),
                    });
                }
                // V2-② Phase A (§6.1/§7): first-comer writes `.bw/project.toml`
                // into the repo as the canonical intent正本. Later-comers are
                // already handled by the sync above (the file exists, values
                // read back). Two paths:
                // - New-repo (owned workspace): write + commit on main (the
                //   repo is ours, no branch protection). Goes with push_head.
                // - Existing-repo (cloned, not owned): write + branch +
                //   PR + Buddy auto-merge (§7 — main may be protected).
                //   project.toml is configuration, not an Issue — auto-merge
                //   here doesn't break "Done 永不自动" (that rule is about
                //   Issues). Issue PRs are never auto-merged (unchanged).
                if !proj.workspace_path.trim().is_empty() && !has_project_toml(&proj.workspace_path)
                {
                    let dir = std::path::Path::new(proj.workspace_path.trim());
                    if let Some(toml) = project_toml_content(&proj) {
                        if bw_engine::workspace::is_owned_workspace(dir).await {
                            // New-repo first-comer: commit directly on main.
                            if let Err(e) = bw_engine::workspace::commit_file(
                                dir,
                                bw_engine::project_file::PROJECT_FILE_REL_PATH,
                                &toml,
                                "chore: project intent (.bw/project.toml)",
                            )
                            .await
                            {
                                self.emit(Event::ActionProgress {
                                    name: "写 project.toml".into(),
                                    state: ActionState::Fail(format!(
                                        "写入 .bw/project.toml 失败:{e}"
                                    )),
                                });
                            }
                        } else if !proj.remote_path.trim().is_empty() {
                            // Existing-repo first-comer: branch + PR + auto-merge.
                            self.write_project_toml_pr(&proj, dir, &toml).await;
                        }
                    }
                }
                // plan/13 D1(#31 记录的缺口):create_repo 只推了首 commit,
                // 创建流途中的章程/组件标准提交停在本地——产品信息正本在
                // 仓里,落地时把 HEAD 一次推齐。失败软降级 toast,不倒灌
                // 创建(remote_path 非空 ⇒ workspace 在 CreateProject 就
                // 已绑定,直接用)。
                if !proj.remote_path.trim().is_empty() && !proj.workspace_path.trim().is_empty() {
                    // plan/14 C14: 落地推送同样是真实网络调用——Started 先
                    // 发,pending→ok/fail 配对到底。
                    let action_name = format!("{} · 落地推送", proj.name);
                    self.emit(Event::ActionProgress {
                        name: action_name.clone(),
                        state: ActionState::Started,
                    });
                    match bw_engine::github::push_head(std::path::Path::new(
                        proj.workspace_path.trim(),
                    ))
                    .await
                    {
                        Ok(()) => {
                            self.emit(Event::ActionProgress {
                                name: action_name,
                                state: ActionState::Ok("已推送".into()),
                            });
                        }
                        Err(e) => {
                            let detail =
                                format!("落地推送失败(章程等提交仍在本地,可稍后手动 git push):{e}");
                            self.emit(Event::ConnectorSynced {
                                name: format!("{} · GitHub", proj.name),
                                ok: false,
                                detail: detail.clone(),
                            });
                            self.emit(Event::ActionProgress {
                                name: action_name,
                                state: ActionState::Fail(detail),
                            });
                        }
                    }
                }
                self.state.view = View::App;
                self.refresh_projects().await?;
                self.refresh_issues().await?;
                // 创建即探活(§6.5 GAP,用户实践点破):建完的连接器就地 probe
                // 一遍——git-repo 顺带喂工作区指标(commits/docs)、codehub/github-
                // repo 翻 Connected。用户不用再为每个项目去 Hub 点「立即同步」。
                // 探活失败软降级留 Error,如实,不倒灌创建(创建本身已落地)。
                let minted: Vec<_> = self
                    .store
                    .list_connectors()
                    .await?
                    .into_iter()
                    .filter(|c| c.project_id == Some(p))
                    .collect();
                for c in minted {
                    let (ok, detail) = self
                        .probe_connector(&c)
                        .await
                        .unwrap_or((false, "探活异常,跳过".into()));
                    let status = if ok {
                        ConnectorStatus::Connected
                    } else {
                        ConnectorStatus::Error
                    };
                    let _ = self
                        .store
                        .set_connector_sync(c.id, status, &run_at_label(now()))
                        .await;
                    self.emit(Event::ConnectorSynced {
                        name: c.name.clone(),
                        ok,
                        detail,
                    });
                }
                self.refresh_connectors().await?;
                self.emit(Event::ConnectorsChanged);
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ViewChanged(View::App));
                self.emit(Event::IssuesChanged);
                // PF1-4: 创建即采一次指标(probe connector 之后,setup 全完成)。
                // cron 不抢跑(见 CreateProject 处 last_run_at=now()),这里补这一
                // 次让新项目总览指标条不再全「—」。best-effort:失败不 block 创建,
                // 不发 Command::CollectMetrics 避免往返/toast——创建流有自己的
                // ActionsBanner 进度(决议 4)。
                let _ = self.collect_project_metrics(p).await;
                // C8 · 末卡「立即让队友开工第一件?」(plan/13 D8): 显式勾选
                // 才跑,默认不跑——不勾是零摩擦的另一半,真的什么都不发生。
                // 勾了就对标配三件套里的①竞品分析显式 dispatch 一次
                // RunIssue(不是 hijack:用户在末卡亲手勾的框)。跑失败只是
                // 一次诚实的失败 toast,不倒灌回 CompleteCreation 本身
                // ——项目创建这件事本身已经落地,不能因为紧跟着的第一次
                // 开工不顺就整体报错(同一份「创建永不因网络失败」的精神,
                // 只是这次是"起跑"而不是"开仓")。
                if run_first {
                    if let Some(issue_id) = first_issue {
                        let session = SessionId::new();
                        self.store
                            .ensure_session(NewSession {
                                id: session,
                                project_id: p,
                                stage_kind: Some(StageKind::Prototype),
                                kind: SessionKind::Create,
                                title: "创建 · 立即开工竞品分析".into(),
                                snippet: String::new(),
                            })
                            .await?;
                        self.state.active_session = Some(session);
                        if let Err(e) = self.run_issue_now(session, issue_id).await {
                            self.emit(Event::WorkflowFailed(format!(
                                "创建流「立即开工」失败,竞品分析活留在可重试状态:{e}"
                            )));
                        }
                    }
                }
            }

            Command::SetWorkspace {
                path,
                allow_commands,
            } => {
                let p = self.active()?;
                let trimmed = path.trim();
                if !trimmed.is_empty() && !std::path::Path::new(trimmed).is_dir() {
                    return Err(AppError::Invalid(format!("工作目录不存在:{trimmed}")));
                }
                self.store.set_workspace(p, trimmed, allow_commands).await?;
                self.refresh_projects().await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::AttachRepo {
                owner,
                repo,
                push_local,
            } => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                let owner = owner.trim().to_string();
                let repo = repo.trim().to_string();
                let owner_repo = format!("{owner}/{repo}");
                let action_name = format!("{} · 接入仓库", proj.name);
                self.emit(Event::ActionProgress {
                    name: action_name.clone(),
                    state: ActionState::Started,
                });

                // 1) 先探活——仓不存在或无权限,如实报错、一个字节不写库
                // (D12 软降级:失败不伪造)。
                if let Err(e) = bw_engine::github::probe_repo(&owner_repo).await {
                    let detail = format!("探活失败:{e}");
                    self.emit(Event::ActionProgress {
                        name: action_name,
                        state: ActionState::Fail(detail.clone()),
                    });
                    return Err(AppError::Invalid(detail));
                }

                // 2) P1-fix(复核 P1 时读回发现的重试死路):项目已有工作区时,
                // 先把本地 origin 接上 —— 必须排在任何写库动作之前。探活通过
                // 之后,`Mismatch`(工作区已挂着别的 origin)是这条命令唯一
                // 还可能失败的分支;若先写了 remote_path/建了 connector 再
                // 撞上 Mismatch,产品侧就没有 UI 重试入口了 ——
                // `AttachRepoCard`(app-desktop/src/screens/op.rs)只在
                // `remote_path` 为空时渲染,写库后卡片消失,用户再点不到
                // 这个动作,只能手改 SQL。先做这步 ⇒ 失败时一个字节都还没
                // 进库,再次调用同一条命令天然就是重试。没有工作区就跳过整步
                // (不是错误——纯身份挂靠,后续 SetWorkspace 再补)。
                let workspace_path = proj.workspace_path.trim().to_string();
                if !workspace_path.is_empty() {
                    let workspace = std::path::Path::new(&workspace_path);
                    if let Err(e) =
                        bw_engine::github::reconcile_local_remote(workspace, &owner, &repo).await
                    {
                        let detail = e.to_string();
                        self.emit(Event::ActionProgress {
                            name: action_name,
                            state: ActionState::Fail(detail.clone()),
                        });
                        return Err(AppError::Invalid(detail));
                    }
                }

                // 3) 写 remote_path / remote_host。
                self.store.set_remote(p, "github.com", &owner_repo).await?;

                // 4) 补建 github-repo connector —— 幂等:同项目已有同 kind
                // 就不重复建(`CreateProject` 的另两条分支都建了它,绑定
                // 分支此前漏了,这里补齐)。
                let has_github_connector = self
                    .store
                    .list_connectors()
                    .await?
                    .iter()
                    .any(|c| c.kind == CONNECTOR_KIND_GITHUB_REPO && c.project_id == Some(p));
                if !has_github_connector {
                    self.store
                        .create_connector(NewConnector {
                            id: ConnectorId::new(),
                            name: format!("{} · GitHub", proj.name),
                            kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                            scope: proj.name.clone(),
                            project_id: Some(p),
                            config: owner_repo.clone(),
                        })
                        .await?;
                }

                // 5) push_local=true 且第 2 步真的跑过(有工作区)时推当前
                // 分支。这一步刻意留在写库之后,不是疏漏:推送失败时
                // remote_path 已经设对是事实正确的(仓确实接上了,只是这
                // 次本地提交没推上去),用户可以自己 `git push` 补,不必也
                // 不该把整条「接仓」判失败重来 —— 那样反而会在下次重试时对
                // 已经正确的 remote_path/connector 做多余的幂等检查。
                if !workspace_path.is_empty() && push_local {
                    let workspace = std::path::Path::new(&workspace_path);
                    let branch = match bw_engine::github::current_branch(workspace).await {
                        Ok(b) => b,
                        Err(e) => {
                            let detail = format!("读取当前分支失败:{e}");
                            self.emit(Event::ActionProgress {
                                name: action_name,
                                state: ActionState::Fail(detail.clone()),
                            });
                            return Err(AppError::Invalid(detail));
                        }
                    };
                    if branch.trim().is_empty() {
                        let detail = "工作区处于 detached HEAD,无法确定要推送的分支".to_string();
                        self.emit(Event::ActionProgress {
                            name: action_name,
                            state: ActionState::Fail(detail.clone()),
                        });
                        return Err(AppError::Invalid(detail));
                    }
                    if let Err(e) = bw_engine::github::push_current_branch(workspace, &branch).await
                    {
                        let detail = format!("推送 {branch} 失败:{e}");
                        self.emit(Event::ActionProgress {
                            name: action_name,
                            state: ActionState::Fail(detail.clone()),
                        });
                        return Err(AppError::Invalid(detail));
                    }
                }

                // 6) 收尾:成功配对 + 刷新。
                self.emit(Event::ActionProgress {
                    name: action_name,
                    state: ActionState::Ok(owner_repo),
                });
                self.refresh_connectors().await?;
                self.refresh_projects().await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::SetClaudeConfig {
                binary,
                max_budget_usd,
                default_mode,
                commands_mode,
            } => {
                if max_budget_usd <= 0.0 {
                    return Err(AppError::Invalid("预算上限必须大于 0".into()));
                }
                self.state.claude_config = ClaudeCliConfig {
                    binary,
                    max_budget_usd,
                    default_mode,
                    commands_mode,
                };
                self.emit(Event::ClaudeConfigChanged);
            }

            Command::LoadVersionLog => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                let result = bw_engine::read_commits(&proj.workspace_path, 30)
                    .await
                    .map_err(|e| e.to_string());
                self.state.version_log = Some((p, result));
                self.emit(Event::VersionLogChanged);
            }

            Command::LoadArtifacts => {
                let p = self.active()?;
                let rows = self.store.list_artifacts(p).await?;
                self.state.artifacts = Some((p, rows));
                self.emit(Event::ArtifactsChanged);
            }

            // L1(plan/11): a real backend function (`cron_effectiveness`)
            // that has existed since the cron-run-attribution work landed but
            // never had a caller — this is that caller.
            Command::LoadCronEffectiveness(id) => {
                let e = self.store.cron_effectiveness(id).await?;
                self.state.cron_effectiveness = Some((id, e));
                self.emit(Event::CronEffectivenessChanged);
            }

            // P4: assemble one Issue's evidence — its runs, what each run
            // really changed (diff between the recorded HEAD pair), and its
            // registered artifacts. Read-only; every number the overlay shows
            // comes from the store / git, nothing synthesized here.
            Command::OpenIssueDetail(id) => {
                let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                let conv = self.store.get_conversation_by_issue(id).await?;
                let is_interactive = conv.is_some();
                // Bug2 半套收齐:有 conversation 行时点卡唤醒与阶段记录
                // SelectSession / ▶跑 同一条 `run_issue_now`(活 PTY 切焦点 /
                // Done·InReview 咨询 / 空 session_id 起手)。旧窄门还要求
                // 「活 PTY | 非空 session_id」,缺行或 hook 未回填时只开弹层
                // 不切终端。无 conversation 行 = 从未跑过,只开证据弹层、
                // 不因点标题误开工(开工仍走 ▶跑)。
                if conv.is_some() {
                    self.run_issue_now(SessionId::new(), id).await?;
                }
                let runs = self.store.list_runs_for_issue(id).await?;
                let artifacts = self.store.list_artifacts_for_issue(id).await?;
                let workspace = self
                    .store
                    .get_project(issue.project_id)
                    .await?
                    .map(|p| p.workspace_path.trim().to_string())
                    .unwrap_or_default();
                let mut changes = Vec::with_capacity(runs.len());
                for r in &runs {
                    let entry = match (&r.head_before, &r.head_after) {
                        (Some(b), Some(a)) if !workspace.is_empty() => {
                            if b == a {
                                Ok(Vec::new())
                            } else {
                                bw_engine::workspace::diff_numstat(&workspace, b, a)
                                    .await
                                    .map(|v| {
                                        v.into_iter()
                                            .map(|c| (c.path, c.added, c.deleted))
                                            .collect::<Vec<_>>()
                                    })
                                    .map_err(|e| format!("对比不可用:{e}"))
                            }
                        }
                        _ => Err("无变更记录(演示模式运行,或早于变更追踪)".to_string()),
                    };
                    changes.push((r.id, entry));
                }
                self.state.issue_detail = Some(IssueDetailData {
                    issue,
                    runs,
                    changes,
                    artifacts,
                    is_interactive,
                });
            }

            Command::CloseIssueDetail => {
                self.state.issue_detail = None;
            }

            Command::CollectArtifacts => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                if proj.workspace_path.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "未配置真实工作区——没有可扫描的代码仓".into(),
                    ));
                }
                let fresh = self
                    .scan_and_register_artifacts(p, &proj.workspace_path, None, None, None)
                    .await?;
                self.emit(Event::ArtifactsRegistered { fresh });
                // Refresh the panel snapshot in the same dispatch so the UI
                // sees the scan's result without a second command.
                let rows = self.store.list_artifacts(p).await?;
                self.state.artifacts = Some((p, rows));
                self.emit(Event::ArtifactsChanged);
            }

            Command::SyncConnector { id } => {
                let all = self.store.list_connectors().await?;
                let c = all
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or(AppError::NotFound)?;
                let (ok, detail) = self.probe_connector(&c).await?;
                let status = if ok {
                    ConnectorStatus::Connected
                } else {
                    ConnectorStatus::Error
                };
                self.store
                    .set_connector_sync(id, status, &run_at_label(now()))
                    .await?;
                self.refresh_connectors().await?;
                self.emit(Event::ConnectorsChanged);
                self.emit(Event::ConnectorSynced {
                    name: c.name.clone(),
                    ok,
                    detail,
                });
            }

            Command::SyncMetricsFile => {
                let p = self.active()?;
                self.sync_metrics_file_for(p).await?;
                // V1 Issue2 Phase 3: keep the command complete — sync
                // connectors.toml alongside metrics.toml (used by
                // collector_demo example + any future manual sync entry
                // point). The UI button that fired this was retired per
                // §3.2/§4 (merge auto-sync covers the normal flow).
                self.sync_connectors_file_for(p).await?;
            }

            Command::SyncRemoteIssues => {
                let p = self.active()?;
                self.sync_remote_issues_for(p, true).await?;
            }

            Command::CollectMetrics => {
                let p = self.active()?;
                let s = self.collect_project_metrics(p).await?;
                // Honest toast: only a pass that really measured at least one
                // metric, with no failures or deferred definitions, is ok.
                // "Nothing collected" must not wear a success colour.
                let ok = s.is_success();
                let mut detail = format!(
                    "采集 · {} 更新 · {} 未变 · {} 未接（legacy 或脚本未产出）",
                    s.changed, s.unchanged, s.deferred
                );
                if let Some(err) = &s.first_error {
                    detail.push_str(&format!(";首个失败:{err}"));
                }
                self.emit(Event::ConnectorSynced {
                    name: "指标采集".into(),
                    ok,
                    detail,
                });
            }

            Command::StartSession {
                id,
                stage_kind,
                kind,
                title,
            } => {
                let p = self.active()?;
                self.store
                    .ensure_session(NewSession {
                        id,
                        project_id: p,
                        stage_kind,
                        kind,
                        title,
                        snippet: String::new(),
                    })
                    .await?;
                self.state.active_session = Some(id);
            }

            Command::RunWorkflow { session, spec } => {
                let p = self.active()?;
                self.run_workflow_inner(p, session, spec, RunTrigger::Manual, None, None, None)
                    .await?;
            }

            Command::RunStagePlaybook {
                session,
                stage_kind,
            } => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                // The baton this stage received — the latest real handoff
                // note (empty on a project's very first stage).
                // `list_handoffs` is newest-first (ORDER BY created_at DESC),
                // so the latest note is `.first()`.
                let handoff_note = self
                    .store
                    .list_handoffs(p)
                    .await?
                    .first()
                    .map(|h| h.note.clone())
                    .unwrap_or_default();
                let workspace_hint = if proj.workspace_path.trim().is_empty() {
                    "（未配置真实工作区 —— 本次运行在 MockExecutor 上，产出仅为流程演示）"
                        .to_string()
                } else {
                    format!(
                        "工作区 {}（git 仓库）。请在其中完成一切产出；之前阶段的产出也在这里，先查看现状再动手。",
                        proj.workspace_path.trim()
                    )
                };
                let ctx = bw_core::playbook::PlaybookCtx {
                    project_name: proj.name.clone(),
                    project_kind: proj.kind.clone(),
                    project_desc: proj.desc.clone(),
                    benchmark: proj.benchmark.clone(),
                    opportunity: proj.opportunity.clone(),
                    north_star: proj.north_star.clone(),
                    ns_def: proj.ns_def.clone(),
                    handoff_note,
                    workspace_hint,
                };
                let spec = stage_workflow_with_playbook(stage_kind, &ctx);
                self.run_workflow_inner(p, session, spec, RunTrigger::Manual, None, None, None)
                    .await?;
            }

            Command::CreateWorkflowSpec {
                id,
                name,
                prompt,
                goal,
                stage_ref,
                phases,
                phase_prompts,
                agents,
                skills,
                loop_config,
                maturity,
                scope,
                source,
                trigger,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                if !phase_prompts.is_empty() && phase_prompts.len() != phases.len() {
                    return Err(AppError::Invalid(
                        "phase_prompts 必须为空或与 phases 等长".into(),
                    ));
                }
                self.store
                    .create_workflow_spec(NewWorkflowSpec {
                        id,
                        name,
                        kind: WorkflowKind::Static {
                            maturity,
                            version: 1,
                            uses: 0,
                            scope,
                            source,
                            trigger,
                        },
                        prompt,
                        goal,
                        stage_ref,
                        // The hub create form is still name-only text editing
                        // (no role-declaration UI yet) — every phase it
                        // authors is honestly `Neutral`. Built-in stage
                        // playbooks are the only source of real roles today
                        // (`bw_core::playbook::phase_metas`).
                        phases: phases.into_iter().map(PhaseMeta::neutral).collect(),
                        phase_prompts,
                        agents,
                        skills,
                        loop_config,
                        // 践行最小切片(2026-07-20):Command 层暂不带 project_id
                        // 参数(那是 P2 全量的事,见 plan/08 §0)——Hub 创建口径
                        // 不变,一律全局。
                        project_id: None,
                        // T16: 创建表单还没有正文录入 UI(那是未来内容创作路径
                        // 的事)——如实留空,不假装有原始文档。
                        content: String::new(),
                    })
                    .await?;
                self.refresh_workflow_specs().await?;
                self.emit(Event::WorkflowSpecsChanged);
            }

            Command::PromoteWorkflow {
                new_id,
                session,
                source,
            } => {
                let p = self.active()?;
                let sess = self
                    .store
                    .list_sessions(p)
                    .await?
                    .into_iter()
                    .find(|s| s.id == session)
                    .ok_or(AppError::NotFound)?;
                let spec = match sess.stage_kind {
                    Some(kind) => stage_workflow(kind),
                    None => {
                        return Err(AppError::Invalid("会话未关联阶段,无法沉淀".into()));
                    }
                };
                self.store.promote_workflow(new_id, &spec, source).await?;
                self.refresh_workflow_specs().await?;
                self.emit(Event::WorkflowSpecsChanged);
            }

            Command::RunHubWorkflow {
                session,
                workflow_id,
            } => {
                let p = self.active()?;
                let spec = self
                    .store
                    .get_workflow_spec(workflow_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                self.store.record_workflow_use(workflow_id).await?;
                self.refresh_workflow_specs().await?;
                self.run_workflow_inner(p, session, spec, RunTrigger::Manual, None, None, None)
                    .await?;
            }

            Command::RunIssue { session, id } => {
                self.run_issue_now(session, id).await?;
            }
            Command::CancelRun { id } => {
                self.cancel_run(id).await?;
            }

            Command::UpdateWorkflowSpec {
                id,
                prompt,
                goal,
                phases,
                phase_prompts,
                agents,
                skills,
                note,
            } => {
                if !phase_prompts.is_empty() && phase_prompts.len() != phases.len() {
                    return Err(AppError::Invalid(
                        "phase_prompts 必须为空或与 phases 等长".into(),
                    ));
                }
                self.store
                    .update_workflow_spec(
                        id,
                        WorkflowEdit {
                            prompt,
                            goal,
                            // Same name-only-text-editing scope as
                            // `CreateWorkflowSpec` above — an "优化" through
                            // this form honestly resets every phase to
                            // `Neutral` (a per-phase role editor is later UI
                            // work, not this ticket).
                            phases: phases.into_iter().map(PhaseMeta::neutral).collect(),
                            phase_prompts,
                            agents,
                            skills,
                            note,
                        },
                    )
                    .await?;
                self.refresh_workflow_specs().await?;
                self.emit(Event::WorkflowSpecsChanged);
            }

            Command::ParseWorkflowContent { workflow_id } => {
                self.parse_workflow_content(workflow_id).await?;
            }

            Command::CreateSkill {
                id,
                name,
                desc,
                category,
                source,
                content,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                // plan/16 §2 防线 1 (S1+S2): a hand-authored skill must be
                // born spec-compliant — the name is the hub-wide join key
                // (SkillRef / agent.skills / 蒸馏溯源), so a bad or
                // ambiguous one spreads.
                guard_skill_name(&name)?;
                // plan/20 R4: Hub 创建落全局池,查重也只查全局池。
                self.guard_skill_name_unique(&name, None, None).await?;
                self.store
                    .create_skill(NewSkill {
                        id,
                        name,
                        // A freshly created skill is honestly "just made,
                        // not yet proven" — Polishing, never Fresh (the
                        // SkillHub/AgentHub UI has no chip for a 3rd tier).
                        maturity: Maturity::Polishing,
                        desc,
                        category,
                        // T7: no stage selector on the hand-authored create
                        // form yet (out of this ticket's scope) — honest
                        // 未归类 until an editor exists to classify it.
                        stages: Vec::new(),
                        stage_origin: StageOrigin::Unclassified,
                        source,
                        content,
                        project_id: None, // Hub 创建口径不变,一律全局
                    })
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::DistillSkillFromIssue {
                skill_id,
                issue_id,
                name,
                desc,
                category,
                content,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                // plan/16 §2 防线 1 (S1+S2): distilled skills are BW 自产 —
                // the compounding loop must mint spec-compliant names from
                // day one (the two legacy Chinese-named rows are exactly the
                // stock this guard prevents regrowing).
                guard_skill_name(&name)?;
                // plan/20 R4: 蒸馏落在源 Issue 所属项目的池,查重只查该池
                // (store::distill_skill_from_issue 按同一 provenance 定归属)。
                let scope = self
                    .store
                    .get_issue(issue_id)
                    .await?
                    .ok_or(AppError::NotFound)?
                    .project_id;
                self.guard_skill_name_unique(&name, Some(scope), None)
                    .await?;
                self.store
                    .distill_skill_from_issue(
                        NewSkill {
                            id: skill_id,
                            name,
                            maturity: Maturity::Polishing,
                            desc,
                            category,
                            // 忽略:store::distill_skill_from_issue 同样改从源
                            // Issue 的真实 stage 派生(T7/2026-08-05,与
                            // project_id 同一 provenance-not-input 规则),
                            // 不采用这里传入的值。
                            stages: Vec::new(),
                            stage_origin: StageOrigin::Unclassified,
                            source: HubSource::SelfBuilt,
                            content,
                            // 忽略:store::distill_skill_from_issue 改从源 Issue
                            // 的真实 project_id 派生归属(provenance),不采用
                            // 这里传入的值。
                            project_id: None,
                        },
                        issue_id,
                    )
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::ImportSkillPackage {
                source_path,
                project_id,
                official_library,
            } => {
                let parsed = skill_import::import_skill_package_from_disk(&source_path)
                    .map_err(AppError::Invalid)?;
                if parsed.name.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "SKILL.md frontmatter 的 name 不能为空".into(),
                    ));
                }
                let source = match official_library {
                    Some(lib) => HubSource::Official {
                        official_library: lib,
                    },
                    None => HubSource::SelfBuilt,
                };
                self.store
                    .import_skill_package(
                        NewSkill {
                            id: SkillId::new(),
                            name: parsed.name,
                            // standards.rs 铁律:maturity 系统派生,新建一律
                            // fresh——外部库再有名,在 BW 里的成熟度只能由
                            // BW 本地真实使用挣出来,不从外部声誉继承。
                            // (/code-review 硬违规修正:原 Mature 引 seed
                            // 先例,但 seed 是内置角色路径,标准未为导入开例外。)
                            maturity: Maturity::Fresh,
                            desc: parsed.desc,
                            // T2 scope: no category assignment on import (no
                            // predetermined classification per plan/12 §2);
                            // stays empty, editable later via `UpdateSkill`.
                            category: String::new(),
                            // T7 (plan/12 §0/§2): no stage guessing on import
                            // either — 未归类 until a human classifies it.
                            stages: Vec::new(),
                            stage_origin: StageOrigin::Unclassified,
                            source,
                            content: parsed.content,
                            project_id,
                        },
                        parsed
                            .files
                            .into_iter()
                            .map(|(rel_path, content)| NewSkillFile { rel_path, content })
                            .collect(),
                    )
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::ImportSkillLibrary {
                root_path,
                official_library,
                project_id,
            } => {
                let dirs =
                    skill_import::find_skill_package_dirs(&root_path).map_err(AppError::Invalid)?;

                // Idempotency key: (name, official_library). Snapshot what
                // already exists in this library once up front, then keep it
                // updated locally as this loop inserts — catches a same-name
                // collision *within* this run too, not just against rows
                // that predate it.
                //
                // T11 (plan/12 §7): a name counts as "already in this
                // library" whether the row is still `Official { lib }` *or*
                // has since been hand-edited and flipped to `SelfBuilt` —
                // `adapted_from` is exactly the surviving `official_library`
                // read-back for that second case (see its doc comment). Only
                // matching the still-`Official` branch (the pre-T11 shape of
                // this filter) would let a re-import mint a brand-new
                // `Official` duplicate of a name the user has since made
                // their own — both an overwrite risk if it raced a later
                // `UpdateSkill` and a same-name-ambiguity risk either way,
                // exactly the two failure modes T11 exists to prevent.
                self.refresh_skills().await?;
                let mut existing_names: std::collections::HashSet<String> = self
                    .state
                    .skills
                    .iter()
                    .filter(|s| {
                        matches!(&s.source, HubSource::Official { official_library: lib } if lib == &official_library)
                            || s.adapted_from.as_deref() == Some(official_library.as_str())
                    })
                    .map(|s| s.name.clone())
                    .collect();

                let mut imported = 0u32;
                let mut skipped = 0u32;
                for dir in dirs {
                    let source_path = dir.to_string_lossy().into_owned();
                    let parsed = skill_import::import_skill_package_from_disk(&source_path)
                        .map_err(AppError::Invalid)?;
                    if parsed.name.trim().is_empty() {
                        return Err(AppError::Invalid(format!(
                            "{source_path}: SKILL.md frontmatter 的 name 不能为空"
                        )));
                    }
                    if existing_names.contains(&parsed.name) {
                        skipped += 1;
                        continue;
                    }
                    self.store
                        .import_skill_package(
                            NewSkill {
                                id: SkillId::new(),
                                name: parsed.name.clone(),
                                // 同 ImportSkillPackage:标准规定新建一律
                                // fresh,成熟度由 BW 本地使用派生。
                                maturity: Maturity::Fresh,
                                desc: parsed.desc,
                                // T3 scope, same as T2: no predetermined
                                // category on import.
                                category: String::new(),
                                // T7: same 未归类-until-classified rule as
                                // `ImportSkillPackage` — no guessing across
                                // 55 imported skills either.
                                stages: Vec::new(),
                                stage_origin: StageOrigin::Unclassified,
                                source: HubSource::Official {
                                    official_library: official_library.clone(),
                                },
                                content: parsed.content,
                                project_id,
                            },
                            parsed
                                .files
                                .into_iter()
                                .map(|(rel_path, content)| NewSkillFile { rel_path, content })
                                .collect(),
                        )
                        .await?;
                    existing_names.insert(parsed.name);
                    imported += 1;
                }

                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
                self.emit(Event::SkillLibraryImported {
                    official_library,
                    imported,
                    skipped,
                });
            }

            Command::UpdateSkill {
                id,
                name,
                desc,
                category,
                content,
                stages,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                // plan/16 §2 防线 1 (S1+S2): renames must land on a
                // spec-compliant, unoccupied name too — this is also the
                // exact door the audit's curated corrections walk through
                // (中文名 → kebab), so the guard and the fix share one rule.
                guard_skill_name(&name)?;
                // T11 (plan/12 §7): "编辑即脱离源头" — an `Official` row whose
                // substantive fields (content/desc/category; `name` is
                // identity, not content) really changed flips to `SelfBuilt`
                // in this same update. Compared against the real pre-edit
                // row, not the caller's own state cache, so a stale UI still
                // decides correctly. A no-op edit (identical content
                // resubmitted) or a rename-only edit never flips.
                let existing = self.store.get_skill(id).await?;
                // plan/20 R4: 改名只在本行所在的池里查重。
                self.guard_skill_name_unique(
                    &name,
                    existing.as_ref().and_then(|s| s.project_id),
                    Some(id),
                )
                .await?;
                let flip_to_self_built = existing.as_ref().is_some_and(|s| {
                    matches!(s.source, HubSource::Official { .. })
                        && (s.content != content || s.desc != desc || s.category != category)
                });
                self.store
                    .update_skill(
                        id,
                        SkillEdit {
                            name,
                            desc,
                            category,
                            content,
                            flip_to_self_built,
                        },
                    )
                    .await?;
                // 归类与内容编辑分两次写:`SkillEdit` 管内容(且带 T11 的
                // flip_to_self_built),归类走 `set_skill_stages`(刻意不碰
                // source/official_library —— 归类是 BW 自己的组织维度,不是对
                // 上游正文的改编,不该让 mattpocock 的 tdd 因为被归到构建段就
                // 失去官方徽记)。
                //
                // Important 修复(2026-08 code review,控制者拍板):编辑表单
                // 的 `picked` 初值就是打开时的现有归类,无论用户有没有碰过五
                // 角色 chip,`save` 都无条件带上 `stages: Some(picked())`——
                // 如果这里对着 `Some(...)` 就无条件 `Manual`,那「只改个错别字
                // 就保存」也会把 `stage_origin` 静默翻成 `Manual`,让这件技能
                // 永久失去静态表自愈资格(2026-08-05 真实日常库 68 件里 65 件
                // 当时是 Table 来源,覆盖面极大)。`stage_origin` 记录的是**归类
                // 动作**的出处,不是「这次请求里带没带 stages 字段」——跟下面
                // `flip_to_self_built` 同一条纪律:比对真实现值,值没变就不算
                // 一次动作,不落 Manual。
                //
                // 集合比较(顺序无关,不能直接 `==` 比 `Vec`——DB 读回按
                // `stage` 升序,不保证等于 UI 勾选顺序):长度相等 + 逐个
                // `contains`。
                if let Some(stages) = stages {
                    let unchanged = existing.as_ref().is_some_and(|e| {
                        e.stages.len() == stages.len()
                            && stages.iter().all(|k| e.stages.contains(k))
                    });
                    // Unclassified 是例外:哪怕请求值(空集)与现值(空集)
                    // 集合相等,「现有 origin 是 Unclassified」时用户提交空集
                    // 仍是一次真实判断——「判定为不属任何阶段」,必须落成
                    // Manual + 零关联行,不能因为集合相等就跳过。
                    let existing_is_unclassified = existing
                        .as_ref()
                        .is_some_and(|e| e.stage_origin == StageOrigin::Unclassified);
                    if !unchanged || existing_is_unclassified {
                        self.store
                            .set_skill_stages(id, &stages, StageOrigin::Manual)
                            .await?;
                    }
                }
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::AdoptIntoProject { target, project_id } => {
                // 语义与守卫见 Command 变体的 doc comment(plan/20 R5)。
                self.store
                    .get_project(project_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                let d = now().date();
                let stamp = format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day());
                // 归属标签:Official 库名是真实上游;其余(自建/会话内)统称
                // 共享目录——不发明更细的出处。
                let origin_of = |source: &HubSource| -> String {
                    match source {
                        HubSource::Official { official_library }
                            if !official_library.is_empty() =>
                        {
                            official_library.clone()
                        }
                        _ => "共享目录".to_string(),
                    }
                };
                match target {
                    AdoptTarget::Skill(sid) => {
                        let src = self.store.get_skill(sid).await?.ok_or(AppError::NotFound)?;
                        if src.project_id.is_some() {
                            return Err(AppError::Invalid(
                                "只能收录共享目录(全局)里的技能——他项目的资产不外借(plan/20 R5)"
                                    .into(),
                            ));
                        }
                        self.guard_skill_name_unique(&src.name, Some(project_id), None)
                            .await?;
                        let files: Vec<NewSkillFile> = self
                            .store
                            .list_skill_files(sid)
                            .await?
                            .into_iter()
                            .map(|f| NewSkillFile {
                                rel_path: f.rel_path,
                                content: f.content,
                            })
                            .collect();
                        let tail = format!("(引入自 {} · {})", origin_of(&src.source), stamp);
                        let desc = if src.desc.trim().is_empty() {
                            tail.clone()
                        } else {
                            format!("{} {}", src.desc.trim(), tail)
                        };
                        self.store
                            .import_skill_package(
                                NewSkill {
                                    id: SkillId::new(),
                                    name: src.name.clone(),
                                    // 新账:成熟度由本项目真实使用挣出来,
                                    // 不从共享行/外部声誉继承(同 Import 先例)。
                                    maturity: Maturity::Fresh,
                                    desc,
                                    category: src.category.clone(),
                                    // 出处保真(R5)延伸到五角色归类:阶段归属
                                    // 与其出处一并原样复制,不因收录进项目而
                                    // 退化成未归类——收录是复制,不是重新判断。
                                    stages: src.stages.clone(),
                                    stage_origin: src.stage_origin,
                                    source: src.source.clone(),
                                    content: src.content.clone(),
                                    project_id: Some(project_id),
                                },
                                files,
                            )
                            .await?;
                        self.refresh_skills().await?;
                        self.emit(Event::SkillsChanged);
                    }
                    AdoptTarget::Agent(aid) => {
                        let src = self.store.get_agent(aid).await?.ok_or(AppError::NotFound)?;
                        if src.project_id.is_some() {
                            return Err(AppError::Invalid(
                                "只能收录共享目录(全局)里的队友——他项目的资产不外借(plan/20 R5)"
                                    .into(),
                            ));
                        }
                        let dup = self
                            .store
                            .list_agents()
                            .await?
                            .iter()
                            .any(|a| a.project_id == Some(project_id) && a.name == src.name);
                        if dup {
                            return Err(AppError::Invalid(format!(
                                "本项目已有同名队友「{}」(plan/20 R4)",
                                src.name
                            )));
                        }
                        // agent 没有 desc 字段可挂尾注;instructions 会整段
                        // 进 prompt,不往里塞出处备注——归属靠 project_id
                        // 徽记 + source 保真,如实不硬造。
                        self.store
                            .create_agent(NewAgent {
                                id: AgentId::new(),
                                name: src.name.clone(),
                                role: src.role.clone(),
                                stage_ref: src.stage_ref,
                                maturity: Maturity::Fresh,
                                skills: src.skills.iter().map(|t| t.name.clone()).collect(),
                                model: src.model.clone(),
                                instructions: src.instructions.clone(),
                                tools: src.tools.clone(),
                                agent_cli: src.agent_cli.clone(),
                                source: src.source.clone(),
                                project_id: Some(project_id),
                            })
                            .await?;
                        self.refresh_agents().await?;
                        self.emit(Event::AgentsChanged);
                    }
                    AdoptTarget::Workflow(wid) => {
                        let src = self
                            .store
                            .get_workflow_spec(wid)
                            .await?
                            .ok_or(AppError::NotFound)?;
                        if src.project_id.is_some() {
                            return Err(AppError::Invalid(
                                "只能收录共享目录(全局)里的工作流——他项目的资产不外借(plan/20 R5)"
                                    .into(),
                            ));
                        }
                        let dup = self
                            .store
                            .list_workflow_specs()
                            .await?
                            .iter()
                            .any(|w| w.project_id == Some(project_id) && w.name == src.name);
                        if dup {
                            return Err(AppError::Invalid(format!(
                                "本项目已有同名工作流「{}」(plan/20 R4)",
                                src.name
                            )));
                        }
                        let tail = format!("(引入自 共享目录 · {stamp})");
                        let goal = if src.goal.trim().is_empty() {
                            tail.clone()
                        } else {
                            format!("{} {}", src.goal.trim(), tail)
                        };
                        // `kind` 原样复制:workflow 的 HubSource 出处随
                        // WorkflowKind::Static 一起走(R5 出处保真);新 id
                        // 意味着 run 史/uses 从零(按 spec id 记账)。
                        self.store
                            .create_workflow_spec(NewWorkflowSpec {
                                id: WorkflowId::new(),
                                name: src.name.clone(),
                                kind: src.kind.clone(),
                                prompt: src.prompt.clone(),
                                goal,
                                stage_ref: src.stage_ref,
                                phases: src.phases.clone(),
                                phase_prompts: src.phase_prompts.clone(),
                                agents: src.agents.clone(),
                                skills: src.skills.clone(),
                                loop_config: src.loop_config.clone(),
                                project_id: Some(project_id),
                                content: src.content.clone(),
                            })
                            .await?;
                        self.refresh_workflow_specs().await?;
                        self.emit(Event::WorkflowSpecsChanged);
                    }
                }
            }

            Command::CreateAgent {
                id,
                name,
                role,
                skills,
                model,
                instructions,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_agent(NewAgent {
                        id,
                        name,
                        role,
                        // T7: no stage selector on the hand-authored create
                        // form yet (out of this ticket's scope) — honest
                        // 通用 until an editor exists to classify it.
                        stage_ref: None,
                        maturity: Maturity::Polishing,
                        skills,
                        model,
                        instructions,
                        // T5 (plan/12 §3): a hand-authored Hub agent declares
                        // no AllowedTools restriction yet (editable later,
                        // same "empty = unset" honesty `ImportSkillPackage`'s
                        // category follows) and runs on the one real executor
                        // this app has; self-authored ⇒ `SelfBuilt`.
                        tools: Vec::new(),
                        agent_cli: "claude-code".to_string(),
                        source: HubSource::SelfBuilt,
                        project_id: None, // Hub 创建口径不变,一律全局
                    })
                    .await?;
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
            }

            Command::UpdateAgent {
                id,
                name,
                role,
                skills,
                model,
                instructions,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                // T11 (plan/12 §7): same flip rule as `UpdateSkill` above.
                // Substantive fields for an Agent are `instructions`/`role`/
                // `model` — the ticket's own list also names `tools`, but
                // `UpdateAgent`/`AgentEdit` carry no `tools` field to edit
                // (AllowedTools isn't wired into this form), so it can never
                // differ through this path and is correctly left out of the
                // comparison. `name`/`skills` (tag list) are identity/
                // structural, not content, same "rename alone doesn't flip"
                // call `UpdateSkill` makes for its own `name`.
                let existing = self.store.get_agent(id).await?;
                let flip_to_self_built = existing.as_ref().is_some_and(|a| {
                    matches!(a.source, HubSource::Official { .. })
                        && (a.instructions != instructions || a.role != role || a.model != model)
                });
                self.store
                    .update_agent(
                        id,
                        AgentEdit {
                            name,
                            role,
                            skills,
                            model,
                            instructions,
                            flip_to_self_built,
                        },
                    )
                    .await?;
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
            }

            Command::ImportAgentDefinition {
                source_path,
                official_library,
            } => {
                let parsed = agent_import::import_agent_definition_from_disk(&source_path)
                    .map_err(AppError::Invalid)?;
                if parsed.name.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "AGENT.md frontmatter 的 name 不能为空".into(),
                    ));
                }
                let source = match &official_library {
                    Some(lib) => HubSource::Official {
                        official_library: lib.clone(),
                    },
                    None => HubSource::SelfBuilt,
                };
                // T11 (plan/12 §7): idempotent re-import, `official_library`
                // path only — see this Command variant's doc comment for why
                // the check lives here rather than in a separate batch
                // command. `adapted_from` catches a name that has since been
                // hand-edited and flipped away from `Official`, exactly the
                // same union `ImportSkillLibrary` now checks.
                let is_duplicate = if let Some(lib) = &official_library {
                    self.refresh_agents().await?;
                    self.state.agents.iter().any(|a| {
                        a.name == parsed.name
                            && (matches!(&a.source, HubSource::Official { official_library: l } if l == lib)
                                || a.adapted_from.as_deref() == Some(lib.as_str()))
                    })
                } else {
                    false
                };
                if !is_duplicate {
                    self.store
                        .create_agent(NewAgent {
                            id: AgentId::new(),
                            name: parsed.name,
                            role: parsed.description,
                            // T7: same 通用-until-classified rule as the Skill
                            // import path — no guessing across 67 ECC agents.
                            stage_ref: None,
                            // 同 ImportSkillPackage:标准规定新建一律 fresh,
                            // 成熟度由 BW 本地真实使用派生,不从外部继承。
                            maturity: Maturity::Fresh,
                            // ECC AGENT.md files don't declare skill tags of
                            // their own; no predetermined mapping (no guessing).
                            skills: Vec::new(),
                            model: parsed.model,
                            instructions: parsed.instructions,
                            tools: parsed.tools,
                            agent_cli: "claude-code".to_string(),
                            source,
                            project_id: None,
                        })
                        .await?;
                }
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
            }

            Command::CreateAutopilotTask {
                id,
                name,
                schedule,
                project_id,
                stage,
                assignee,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_cron_task(NewCronTask {
                        id,
                        name,
                        target: String::new(), // 历史字段,建活模式不用
                        schedule,
                        project_id,
                        mode: CronMode::CreateIssue,
                        issue_stage: stage,
                        issue_assignee: assignee,
                        last_run_at: None,
                    })
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::SetCronStatus { id, status } => {
                self.store.set_cron_status(id, status).await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::CreateConnector {
                id,
                name,
                kind,
                scope,
                project_id,
                config,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_connector(NewConnector {
                        id,
                        name,
                        kind,
                        scope,
                        project_id,
                        config,
                    })
                    .await?;
                self.refresh_connectors().await?;
                self.emit(Event::ConnectorsChanged);
            }

            Command::CreateKnowledgeSource {
                id,
                name,
                kind,
                used_by,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_knowledge_source(NewKnowledgeSource {
                        id,
                        name,
                        kind,
                        used_by,
                    })
                    .await?;
                self.refresh_knowledge_sources().await?;
                self.emit(Event::KnowledgeSourcesChanged);
            }

            Command::CreateIssue {
                id,
                stage,
                title,
                desc,
                priority,
                standard_skill,
            } => {
                let p = self.active()?;
                if title.trim().is_empty() {
                    return Err(AppError::Invalid("标题不能为空".into()));
                }
                // P3: pass the slug through as-is — no validation that could
                // fail issue creation over it. `standard_skill_block` (run
                // time) already resolves an unknown/content-less slug to an
                // honest no-op, so a typo or a since-deleted skill here is
                // never a reason to reject the issue.
                self.store
                    .create_issue(NewIssue {
                        id,
                        project_id: p,
                        stage,
                        title: title.clone(),
                        desc: desc.clone(),
                        priority,
                        standard_skill,
                    })
                    .await?;
                // C4: 项目挂了 GitHub 仓时,建单同时经 gh 真开一个 GitHub
                // issue;remote_path 为空的项目在这里直接短路返回,今天的
                // 行为一个字节不变。announce=false(plan/14 C14 范围收敛):
                // op 面板的手动建单不在本票覆盖范围,行为一个字节不变。
                self.sync_issue_to_github(p, id, &title, &desc, false)
                    .await?;
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::TransitionIssue { id, status } => {
                // Read the prior state first: the accounting below must fire
                // exactly once per work item, on its FIRST …→Done edge.
                // `settled_at` is the persistent settle-once marker — without
                // it, a Done → reopen → Done bounce (reachable through this
                // public command even though the desktop only offers forward
                // moves) would credit the same work twice.
                let prev = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // A5-F: `Blocked` has its own entry point (`BlockIssue`) that
                // forces a reason — bare `TransitionIssue` never reaches it,
                // even though the edge is graph-legal (`can_transition_to`
                // says so); this command-level rule sits on top of the table.
                if status == IssueStatus::Blocked {
                    return Err(AppError::Invalid(format!(
                        "#{} 转 Blocked 需要阻塞原因;请使用 BlockIssue 命令",
                        prev.number
                    )));
                }
                // A re-dispatch of the SAME status (e.g. a duplicated Done
                // command) is a harmless re-affirmation, not a transition —
                // `can_transition_to` has no self-loops by design, so it's
                // checked only for a genuine state change. The settle-once
                // guard below (keyed on `prev.status != Done`) already makes
                // this safe: re-affirming Done fires no accounting twice.
                if status != prev.status && !prev.status.can_transition_to(status) {
                    return Err(AppError::Invalid(format!(
                        "非法转移:#{} {}→{}",
                        prev.number,
                        prev.status.label(),
                        status.label()
                    )));
                }
                self.store.transition_issue(id, status).await?;
                let newly_done = status == IssueStatus::Done
                    && prev.status != IssueStatus::Done
                    && prev.settled_at.is_none();
                if newly_done {
                    let issue = prev;
                    self.store
                        .mark_issue_settled(id, now().unix_timestamp())
                        .await?;
                    // The Done edge is the issue-side settle: the same real
                    // accounting a workflow-run settle does, fed by the same
                    // store functions. An issue completed by an agent teammate
                    // is one real run + one real win for that agent —
                    // `win_rate` derives from these counters, never hand-set.
                    // (Cancelled records nothing: dropping an issue is not
                    // evidence about the agent's work, and inventing a loss
                    // would fabricate a metric. Reopen-and-redo also records
                    // nothing new: one work item, one credit — the first win
                    // stands in the append-only history.)
                    if let Some(agent_id) = issue.assignee {
                        if let Some(agent) = self.store.get_agent(agent_id).await? {
                            // plan/20 R3: by-id——此前按 name 全表 UPDATE,
                            // W1 之后每个项目都有同名五角色副本,一次 Done
                            // 会给所有项目的同名队友齐记战绩(真 bug)。
                            self.store.record_agent_run(agent.id, true).await?;
                            self.refresh_agents().await?;
                            self.emit(Event::AgentsChanged);
                        }
                    }
                    // Bug1(V1-TermDemote):issue → Done。若交付 PTY 仍活
                    // (claude 提完 MR 往往不退出),降级为咨询 → 放 active_run
                    // 锁,同项目别的 issue 可跑。PTY + worktree 留着(不杀、不
                    // 清)。MergeIssuePr 内部 dispatch 到这里,自动覆盖。PTY 已
                    // 死则 no-op,让待处理 settle 正常 finalize。降级失败不阻塞
                    // Done 记账(降级是放锁,best-effort)。
                    let _ = self.demote_delivery_to_consultation(id).await;
                    // P4 (2026-08-06 cowelink 验证 §2.3/§5): 「已完成」是唯一的
                    // 验收兜底,不管走的是 `MergeIssuePr`(内部 dispatch 到这里)
                    // 还是网页上把 PR 合了、回 buddy 裸点「→已完成」——两条路都
                    // 该把远端可能已经合入的改动拉回本地、把 `.bw/metrics.toml`/
                    // `.bw/connectors.toml` 正本同步进 SQLite 缓存。此前只有
                    // `MergeIssuePr` 路径做这件事,裸 `TransitionIssue`(网页合
                    // MR 场景)完全跳过 sync,业务指标停在 seed 值——这是本条
                    // 验证日志的头号发现。挪到这唯一的 Done 记账口后,两条入口
                    // 共用同一次 pull+sync,不重复跑(`MergeIssuePr` 内部通过
                    // `dispatch` 到这里,不再自己另跑一遍)。收拢/同步失败只软
                    // 降级 toast,不因此回滚已经发生的验收。
                    if let Ok(Some(proj)) = self.store.get_project(issue.project_id).await {
                        if !proj.workspace_path.trim().is_empty()
                            && !proj.remote_path.trim().is_empty()
                        {
                            match bw_engine::github::sync_default_branch(std::path::Path::new(
                                proj.workspace_path.trim(),
                            ))
                            .await
                            {
                                Ok(()) => {
                                    self.sync_metrics_file_for(issue.project_id).await?;
                                    self.sync_connectors_file_for(issue.project_id).await?;
                                }
                                Err(e) => {
                                    self.emit(Event::ConnectorSynced {
                                        name: "metrics.toml".into(),
                                        ok: false,
                                        detail: format!(
                                            "验收后工作区收拢默认分支失败,指标/连接器正本未同步(可手动重试):{e}"
                                        ),
                                    });
                                }
                            }
                        }
                        // Artifact reflux, issue-scoped: whatever real files
                        // exist in the workspace at completion time get
                        // registered against the issue's stage (idempotent —
                        // an unchanged workspace registers 0 fresh rows). Runs
                        // after the pull above so a webMerge-completed issue's
                        // artifact scan sees the just-pulled files, not stale
                        // pre-pull state.
                        if !proj.workspace_path.trim().is_empty() {
                            if let Ok(fresh) = self
                                .scan_and_register_artifacts(
                                    issue.project_id,
                                    &proj.workspace_path,
                                    None,
                                    Some(issue.stage),
                                    Some(id),
                                )
                                .await
                            {
                                if fresh > 0 {
                                    self.emit(Event::ArtifactsRegistered { fresh });
                                }
                            }
                        }
                    }
                    // A4: feed the stage's machine "完成 Issue 数" metric —
                    // change-guarded; empty target ⇒ Unknown (no fake green).
                    self.feed_stage_done_count(issue.project_id, issue.stage)
                        .await?;
                }
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::MergeIssuePr { id } => {
                let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // Idempotent short-circuit BEFORE any gh call: an already-Done
                // / already-settled Issue is a no-op — a re-dispatch never
                // re-merges (so `gh pr merge` stays called exactly once) and
                // never re-accounts (settle-once already stands).
                if issue.status == IssueStatus::Done || issue.settled_at.is_some() {
                    self.emit(Event::ConnectorSynced {
                        name: format!("#{} · merge", issue.number),
                        ok: true,
                        detail: "已完成,无需重复合入".into(),
                    });
                    return Ok(());
                }
                if issue.pr_number == 0 {
                    return Err(AppError::Invalid(format!(
                        "#{} 没有 PR,无法 merge;无 PR 的活用 TransitionIssue 显式完成",
                        issue.number
                    )));
                }
                // 评审中由开放 PR 派生 (D3): only an InReview PR issue is
                // merge-acceptable.
                if issue.status != IssueStatus::InReview {
                    return Err(AppError::Invalid(format!(
                        "#{} 处于{},不在评审中,不能 merge",
                        issue.number,
                        issue.status.label()
                    )));
                }
                let proj = self
                    .store
                    .get_project(issue.project_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                if proj.remote_path.trim().is_empty() {
                    return Err(AppError::Invalid(format!(
                        "#{} 的项目未挂远端仓,无法 merge PR/MR",
                        issue.number
                    )));
                }
                // Immediate feedback while `merge_mr` awaits (often several
                // seconds). Event forwarder runs concurrent with dispatch, so
                // the toast appears before Done lands — pairs with the UI
                // button busy state (Vm only rebuilds after this command).
                self.emit(Event::ConnectorSynced {
                    name: format!("#{} · merge", issue.number),
                    ok: true,
                    detail: format!("正在合入 PR/MR #{}…", issue.pr_number),
                });
                // merge PR/MR — the human验收 action, the ONLY place a merge is
                // ever called (never from any executor/run path; plan/13
                // D3+D11). Routed through the Remote factory (bug③ 2026-07-30):
                // codehub projects merge via `codehub-cli mr merge`, github via
                // `gh pr merge` — before this, MergeIssuePr crashed `gh pr
                // merge` on codehub.
                let merge_result = match bw_engine::remote::Remote::for_project(
                    &proj.provider,
                    &proj.remote_host,
                    &proj.remote_path,
                ) {
                    Ok(r) => r.merge_mr(issue.pr_number).await,
                    Err(e) => Err(e),
                };
                if let Err(e) = merge_result {
                    // 绝不反向改写:a merge failure (including the drift case of
                    // a PR/MR already merged on the web) is only reflected — the
                    // Issue stays InReview and retryable, nothing is settled.
                    self.emit(Event::ConnectorSynced {
                        name: format!("#{} · merge", issue.number),
                        ok: false,
                        detail: format!("merge PR/MR #{} 失败,活留在评审中:{e}", issue.pr_number),
                    });
                    return Ok(());
                }
                // Settle Done through the EXISTING TransitionIssue InReview→Done
                // path — settle-once accounting reused verbatim, no second
                // accounting path. (Box::pin: `dispatch` recurses into itself;
                // TransitionIssue never re-enters MergeIssuePr, so it's bounded.)
                Box::pin(self.dispatch(Command::TransitionIssue {
                    id,
                    status: IssueStatus::Done,
                }))
                .await?;
                // issue 关闭是 merge 的后果. github: `Closes #<n>` should have
                // closed it; verify + 补关 idempotently if GitHub didn't.
                // codehub: the MR body's `Closes #<n>` auto-closes on merge
                // (GitLab standard, set in `codehub::create_mr`); `--issue-nums`
                // only links, doesn't auto-close (2026-07-31 实测). So codehub
                // issues close via the MR body, not this gh-only补关 block —
                // skip it for codehub (never reopen, never fight drift). Bug③:
                // was unconditional → `gh issue` crashed on codehub; now github-only.
                if matches!(proj.provider.as_str(), "github" | "") {
                    let remote = proj.remote_path.trim().to_string();
                    match bw_engine::github::issue_state(&remote, issue.github_number).await {
                        Ok(state) if state.eq_ignore_ascii_case("OPEN") => {
                            if let Err(e) =
                                bw_engine::github::close_issue(&remote, issue.github_number).await
                            {
                                self.emit(Event::ConnectorSynced {
                                    name: format!("#{} · 关单", issue.number),
                                    ok: false,
                                    detail: format!("PR 已 merge,但补关 GitHub issue 失败:{e}"),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                self.emit(Event::ConnectorSynced {
                    name: format!("#{} · 验收", issue.number),
                    ok: true,
                    detail: format!(
                        "已 merge PR #{},#{} 验收完成",
                        issue.pr_number, issue.number
                    ),
                });
                // P4 (2026-08-06): pull-default-branch + sync metrics/
                // connectors 正本已经挪到 `TransitionIssue` 的 Done 记账口
                // (它就在上面几行通过 `dispatch` 走过了)——两个入口(这里的
                // MergeIssuePr 和网页合 MR 后裸点「→已完成」)现在共用同一次
                // sync,这里不再重复跑一遍。
            }

            Command::AssignIssue { id, assignee } => {
                // plan/20 R1 命令层守卫:他项目的队友与种A(工作区登记行)
                // 不可指派——UI 池已收窄,这里把口子在命令层锁死(深链/
                // 指挥器路径同受约束)。全局共享行仍可指派:存量流程与
                // examples 不破坏,战绩按 R3 记到被指派的那一行,不污账。
                if let Some(aid) = assignee {
                    let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                    let agent = self
                        .store
                        .get_agent(aid)
                        .await?
                        .ok_or_else(|| AppError::Invalid("指派对象不存在".into()))?;
                    if let Some(owner) = agent.project_id {
                        if owner != issue.project_id {
                            return Err(AppError::Invalid(format!(
                                "「{}」是别的项目的队友,不可跨项目指派(plan/20 R1)",
                                agent.name
                            )));
                        }
                    }
                    if agent.source.is_project_assets() {
                        return Err(AppError::Invalid(format!(
                            "「{}」是工作区登记行(种A),仅登记可见,不可指派",
                            agent.name
                        )));
                    }
                }
                self.store.assign_issue(id, assignee).await?;
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::BlockIssue { id, reason } => {
                let reason = reason.trim().to_string();
                if reason.is_empty() {
                    return Err(AppError::Invalid("转 Blocked 必须给出阻塞原因".into()));
                }
                let prev = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // Same table as TransitionIssue queries — Blocked is only
                // reachable from Todo/InProgress/InReview (`can_transition_to`
                // is the single source of truth for both entry points).
                if !prev.status.can_transition_to(IssueStatus::Blocked) {
                    return Err(AppError::Invalid(format!(
                        "非法转移:#{} {}→阻塞",
                        prev.number,
                        prev.status.label()
                    )));
                }
                self.store.block_issue(id, &reason).await?;
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::SendSessionMessage { session, text } => {
                self.store
                    .append_message(session, Author::Builder, &text)
                    .await?;
                self.emit(Event::SessionMessageAdded {
                    session,
                    role: Author::Builder,
                    text: text.clone(),
                });
                // Deterministic mock reply (the real agent reply arrives via Tier C).
                let reply = format!("【mock】已收到:{text}");
                self.store
                    .append_message(session, Author::Agent, &reply)
                    .await?;
                self.emit(Event::SessionMessageAdded {
                    session,
                    role: Author::Agent,
                    text: reply,
                });
            }

            Command::OpenProject(id) => {
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                self.state.active_project = Some(id);
                self.state.active_session = None;
                self.state.panel = Panel::Progress;
                self.state.scope = Scope::All;
                self.state.view = match proj.phase {
                    Readiness::ColdStart => View::Create,
                    Readiness::Running => {
                        // Freshness is clock-relative — re-derive on open so a
                        // value that went stale since last time shows as such.
                        self.store.recompute_signals(id, now()).await?;
                        self.refresh_projects().await?;
                        View::App
                    }
                };
                self.refresh_issues().await?;
                self.emit(Event::ViewChanged(self.state.view));
            }

            Command::DeleteProject(id) => {
                // W3-9: cache the project row before delete_project wipes it,
                // then judge whether its workspace is a buddy-built clone
                // (under workspaces_root, named <slug>-<uuid8hex>) — only
                // those get removed. A user-bound pre-existing directory is
                // never touched (the user may want to keep the evidence).
                // Order is DB-first: delete_project succeeds, then best-effort
                // remove_dir_all — a dir left behind is a manual cleanup, the
                // reverse (dir gone, DB row still pointing at it) is worse.
                let cached_proj = self.store.get_project(id).await?;
                self.store.delete_project(id).await?;
                if let Some(proj) = cached_proj {
                    if is_buddy_built_clone(
                        &proj.workspace_path,
                        &proj.name,
                        id,
                        self.workspaces_root.as_deref(),
                    ) {
                        if let Err(e) = std::fs::remove_dir_all(std::path::Path::new(
                            proj.workspace_path.trim(),
                        )) {
                            // DB row is already gone — a leftover dir is a
                            // manual cleanup, not a data-integrity break.
                            // Loud, honest, non-fatal.
                            eprintln!(
                                "W3-9: 删项目 {} 的 buddy 自建工作目录失败(目录残留,可手动删): {}",
                                proj.name, e
                            );
                        }
                    }
                }
                if self.state.active_project == Some(id) {
                    self.state.active_project = None;
                    self.state.active_session = None;
                    self.state.view = View::Projects;
                    self.emit(Event::ViewChanged(View::Projects));
                }
                self.refresh_projects().await?;
                self.emit(Event::ProjectsChanged);
            }

            Command::DeleteSession(id) => {
                self.store.delete_session(id).await?;
                // If the deleted session was the chat-focused one, drop the
                // stale pointer so the workflow panel doesn't try to render
                // messages for a session that no longer exists.
                if self.state.active_session == Some(id) {
                    self.state.active_session = None;
                }
                // review: refresh issues so state.issues doesn't hold a stale
                // session_id ref (delete_session nulled it in the DB; this
                // re-reads the honest NULL into state).
                self.refresh_issues().await?;
                self.emit(Event::ViewChanged(self.state.view));
            }

            Command::BackToProjects => {
                self.state.view = View::Projects;
                self.state.active_project = None;
                self.state.active_session = None;
                self.refresh_projects().await?;
                self.refresh_issues().await?;
                self.emit(Event::ViewChanged(View::Projects));
            }

            Command::SetPanel(p) => self.state.panel = p,
            Command::SetScope(s) => self.state.scope = s,
            Command::SelectSession(s) => {
                self.state.active_session = s;
                // Bug2(V1-TermFocus)+收口补洞:左→终端走 run_issue_now(与看板
                // ▶跑/续聊等价)。错误上浮 → kernel 打 UiNote::Error,不再静默
                // 吞掉(旧 `let _ =` 让用户只看见空工作流区、无任何提示)。
                // 解析不到 issue(纯阶段循环会话)→ sync 内 early Ok,只亮高亮。
                if let Some(sid) = s {
                    self.sync_session_to_terminal(sid).await?;
                }
                // 侧栏切会话时收起看板证据弹层——弹层曾用 fixed inset:0 盖住
                // 整窗(含侧栏),关掉后若 state 仍挂着 issue_detail,回 Issue
                // 面板会再次挡住侧栏;切会话即清,两侧导航一致。
                self.state.issue_detail = None;
            }
            // V1 终端会话重构·底座: 按 conversation_id 转发到 TerminalManager。
            Command::TerminalInput {
                conversation_id,
                bytes,
            } => {
                let _ = self
                    .state
                    .terminal_manager
                    .input(conversation_id, bw_engine::PtyInput::Bytes(bytes));
            }
            Command::TerminalResize {
                conversation_id,
                cols,
                rows,
            } => {
                // 无活连接时也记 fit 尺寸,供下次 attach 用真值而非 80×24。
                if !self
                    .state
                    .terminal_manager
                    .resize(conversation_id, cols, rows)
                {
                    self.state.terminal_manager.note_fit_size(cols, rows);
                }
            }
        }
        Ok(())
    }
}
