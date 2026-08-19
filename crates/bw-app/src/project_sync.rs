//! 项目与外部世界的同步:工作区探测、GitHub/CodeHub issue 双向、连接器探针、项目资产、产物登记。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// 五角色归类的 Boot 对账(2026-08-05)。三条来源按优先级递增:
    ///
    /// 1. **静态表**(`bw_core::stage_catalog`):随包/vendored 技能的归类
    ///    正本。按名对账,幂等 —— 与库中现值不同就改齐(这是自愈:表改了、库
    ///    跟上)。
    /// 2. **蒸馏派生**:有 `distilled_from_issue` 的技能,按出处 Issue 的 stage
    ///    归类。这正是 `distilled_skills_block` 今天已在用的口径,不新造判据。
    /// 3. **人工覆盖**(`StageOrigin::Manual`):整条跳过,永不回填。
    ///
    /// 不在静态表、也没有蒸馏出处的技能如实留在「未归类」——绝不猜。
    ///
    /// `StageOrigin::Legacy`(搬自已删除的旧 `skill.stage_ref` 单值列,原始
    /// 出处不可考,真实案例 `metrics-render`)在下面**没有专门的跳过分支**,
    /// 是推出来的结论、不是漏掉的分支:保值搬迁
    /// (`bw_store::sqlite::migrate_legacy_skill_stage_ref`)只在
    /// `stages_for(name)` 返回 `None` 时才把一行标成 `Legacy`——换句话说,
    /// 每一条 `Legacy` 行按定义就是静态表覆盖不到的行,所以这里对同一件
    /// 技能重新查 `stages_for(&s.name)` 必然还是 `None`,自然走不进下面的
    /// 静态表分支;而蒸馏分支的门槛是 `stage_origin == Unclassified`,
    /// `Legacy` 也不满足。两条分支天然都放过它,值原样不动。
    pub(crate) async fn reconcile_skill_stages(&self) -> Result<(), AppError> {
        let by_id = self.store.list_skill_stages().await?;
        for s in self.store.list_skills().await? {
            // 人工归过类的行是终点,任何自动来源都不许覆盖。
            if s.stage_origin == StageOrigin::Manual {
                continue;
            }
            if let Some(want) = bw_core::stage_catalog::stages_for(&s.name) {
                let have = by_id.get(&s.id).map(Vec::as_slice).unwrap_or(&[]);
                // 已经一致就不写 —— 每次 Boot 空转一遍 UPDATE 会白白推高
                // updated_at,让「这行最近被动过」这个信号失真。
                let same = have.len() == want.len() && want.iter().all(|k| have.contains(k));
                if !(same && s.stage_origin == StageOrigin::Table) {
                    self.store
                        .set_skill_stages(s.id, want, StageOrigin::Table)
                        .await?;
                }
                continue;
            }
            // 蒸馏技能:出处 Issue 的 stage 就是它的阶段。出处 Issue 查不到
            // (理论上不该发生)如实跳过,不编一个阶段出来。
            if s.stage_origin == StageOrigin::Unclassified {
                if let Some(iid) = s.distilled_from_issue {
                    if let Some(issue) = self.store.get_issue(iid).await? {
                        self.store
                            .set_skill_stages(s.id, &[issue.stage], StageOrigin::Distilled)
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Run one connector's real probe. Returns `(healthy, honest detail)`;
    /// errors only on kinds that have no real probe (there is no fake
    /// "synced" for those) or store failures.
    pub(crate) async fn probe_connector(
        &mut self,
        c: &Connector,
    ) -> Result<(bool, String), AppError> {
        match c.kind.as_str() {
            CONNECTOR_KIND_GIT_REPO => {
                // The bound project's *current* workspace is the live truth;
                // `config` is the provisioning-time record / fallback.
                let workspace = match c.project_id {
                    Some(p) => self
                        .store
                        .get_project(p)
                        .await?
                        .map(|proj| proj.workspace_path)
                        .filter(|w| !w.trim().is_empty())
                        .unwrap_or_else(|| c.config.clone()),
                    None => c.config.clone(),
                };
                match evidence::collect(&workspace).await {
                    Ok(ev) => {
                        // Tier D for real: the probe's numbers flow into the
                        // bound project's matching metrics as machine-source
                        // observations (only when the metric exists and the
                        // value really changed — no observation spam).
                        if let Some(p) = c.project_id {
                            self.feed_workspace_metrics(p, &ev).await?;
                            self.sync_project_assets(p, &workspace).await;
                        }
                        Ok((
                            true,
                            format!(
                                "{} 提交 · {} 追踪文件 · {} 文档 · {} 未提交路径",
                                ev.commit_count, ev.tracked_files, ev.docs_files, ev.dirty_paths
                            ),
                        ))
                    }
                    Err(e) => Ok((false, e.to_string())),
                }
            }
            CONNECTOR_KIND_CLAUDE_CLI => {
                let binary = if c.config.trim().is_empty() {
                    self.state
                        .claude_config
                        .binary
                        .clone()
                        .unwrap_or_else(|| "claude".into())
                } else {
                    c.config.trim().to_string()
                };
                match claude_version_probe(&binary).await {
                    Ok(v) => Ok((true, v)),
                    Err(e) => Ok((false, e)),
                }
            }
            CONNECTOR_KIND_GITHUB_REPO => {
                // plan/13 D12: 真探针一次,探不通如实 false,绝不伪造「已同步」。
                // 经 Remote 分发:github-repo 连器 → Remote::Github(config);
                // codehub-repo 连器(P4)另有 arm → Remote::Codehub{host,path}。
                let cfg = c.config.trim();
                if cfg.is_empty() {
                    return Ok((false, "连接器未记录 owner/repo,无法探活".into()));
                }
                let remote = bw_engine::remote::Remote::Github(cfg.to_string());
                match remote.probe().await {
                    Ok(detail) => Ok((true, detail)),
                    Err(e) => Ok((false, e.to_string())),
                }
            }
            CONNECTOR_KIND_CODEHUB_REPO => {
                // codehub 真探针:`codehub-cli project view` 经 Remote::Codehub。
                // config = "host/path"(创建时 mint 进来),split_once('/') 拆出
                // host + path(path 自带 /,只切第一段)。探不通如实 false。
                let cfg = c.config.trim();
                let Some((host, path)) = cfg.split_once('/') else {
                    return Ok((false, "codehub 连器 config 需 'host/path',无法探活".into()));
                };
                let remote = bw_engine::remote::Remote::Codehub {
                    host: host.to_string(),
                    path: path.to_string(),
                };
                match remote.probe().await {
                    Ok(detail) => Ok((true, detail)),
                    Err(e) => Ok((false, e.to_string())),
                }
            }
            CONNECTOR_KIND_SCRIPT => {
                // plan18-③:探活=检查项目仓里脚本文件在位(不真跑,真跑留
                // collect arm,避免探活就触发长脚本)。config=JSON
                // {script,output,command}。
                let workspace = match c.project_id {
                    Some(p) => self
                        .store
                        .get_project(p)
                        .await?
                        .map(|proj| proj.workspace_path)
                        .filter(|w| !w.trim().is_empty())
                        .unwrap_or_default(),
                    None => String::new(),
                };
                if workspace.trim().is_empty() {
                    return Ok((false, "script 连接器需项目工作区,无法探活".into()));
                }
                let cfg: ScriptConnectorConfig =
                    match ScriptConnectorConfig::from_config(c.config.trim()) {
                        Ok(v) => v,
                        Err(e) => return Ok((false, format!("script 连接器 config 解析失败:{e}"))),
                    };
                if cfg.script.trim().is_empty() {
                    return Ok((false, "script 连接器未记录脚本路径".into()));
                }
                if Path::new(cfg.script.trim()).is_absolute() {
                    return Ok((false, "script 连接器 config 用绝对路径,需相对工作区".into()));
                }
                let script_path = Path::new(&workspace).join(&cfg.script);
                match std::fs::metadata(&script_path) {
                    Ok(_) => Ok((
                        true,
                        format!("脚本 {} 在位(输出 {})", cfg.script, cfg.output),
                    )),
                    Err(e) => Ok((
                        false,
                        format!("脚本 {} 不存在:{}", script_path.display(), e),
                    )),
                }
            }
            other => Err(AppError::Invalid(format!(
                "连接器类型「{other}」没有真实探针——不支持同步(诚实拒绝,不伪造状态)"
            ))),
        }
    }

    /// plan/渠道6 · sync a project's own `skills/` + `agents/` from its
    /// workspace into the hub as 种A (registered-visible, never injected).
    /// Re-scanned on every git-repo connector probe (build-time probe-at-
    /// creation + manual「立即同步」). 项目仓是正本, buddy DB is a mirror, so
    /// this is a **full rebuild** of the project's project-assets batch:
    /// delete the existing batch then re-import everything on disk. The
    /// delete gate is `project_id == project && source.is_project_assets()`
    /// — never touches distilled `SelfBuilt` rows or global libraries.
    ///
    /// 种A rows have no consumers (excluded from every injection picker via
    /// `is_project_assets`) and `uses`/`runs` stay 0, so the id churn from
    /// delete+recreate is harmless. Soft-degrade: every store error is
    /// logged to stderr (`[BW]`) and swallowed — scanning skills is a side-
    /// product of the connector probe and must never block the主功能 (git
    /// evidence + workspace metrics). A missing `skills/`/`agents/` dir is a
    /// normal empty scan, not an error.
    pub(crate) async fn sync_project_assets(&mut self, project: ProjectId, workspace: &str) {
        let source = HubSource::Official {
            official_library: BW_PROJECT_ASSETS_LIBRARY.to_string(),
        };

        // ── skills: full rebuild (delete the project-assets batch, re-import scanned) ──
        // `import_skill_package` (not `create_skill`) so the folder's support
        // files land in `skill_file` too — `create_skill` would drop the file
        // tree. Maas skills carry references/scripts/knowledge dirs.
        let scanned_skills = skill_import::scan_project_skills_dir(workspace);
        let existing_skills: Vec<SkillCard> = match self.store.list_skills().await {
            Ok(all) => all
                .into_iter()
                .filter(|s| s.project_id == Some(project) && s.source.is_project_assets())
                .collect(),
            Err(e) => {
                eprintln!("[BW] sync_project_assets: list_skills 失败,跳过 skill 同步:{e}");
                return;
            }
        };
        for s in &existing_skills {
            if let Err(e) = self.store.delete_skill(s.id).await {
                eprintln!(
                    "[BW] sync_project_assets: delete_skill「{}」失败:{e}",
                    s.name
                );
            }
        }
        for pkg in &scanned_skills {
            if let Err(e) = self
                .store
                .import_skill_package(
                    NewSkill {
                        id: SkillId::new(),
                        name: pkg.name.clone(),
                        // 同 ImportSkillPackage:外部资产,成熟度由本地真实使用派生,不从外部声誉继承。
                        maturity: Maturity::Fresh,
                        desc: pkg.desc.clone(),
                        category: String::new(),
                        // T7 一贯的 通用-until-classified:项目工作区扫描同样
                        // 不猜阶段。
                        stages: Vec::new(),
                        stage_origin: StageOrigin::Unclassified,
                        source: source.clone(),
                        content: pkg.content.clone(),
                        project_id: Some(project),
                    },
                    pkg.files
                        .iter()
                        .map(|(rel_path, content)| NewSkillFile {
                            rel_path: rel_path.clone(),
                            content: content.clone(),
                        })
                        .collect(),
                )
                .await
            {
                eprintln!(
                    "[BW] sync_project_assets: import skill「{}」失败:{e}",
                    pkg.name
                );
            }
        }

        // ── agents: full rebuild (single AGENT.md files, no skill_file equivalent) ──
        let scanned_agents = agent_import::scan_project_agents_dir(workspace);
        let existing_agents: Vec<AgentCard> = self
            .store
            .list_agents()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a.project_id == Some(project) && a.source.is_project_assets())
            .collect();
        for a in &existing_agents {
            if let Err(e) = self.store.delete_agent(a.id).await {
                eprintln!(
                    "[BW] sync_project_assets: delete_agent「{}」失败:{e}",
                    a.name
                );
            }
        }
        for def in &scanned_agents {
            if let Err(e) = self
                .store
                .create_agent(NewAgent {
                    id: AgentId::new(),
                    name: def.name.clone(),
                    role: def.description.clone(),
                    stage_ref: None,
                    maturity: Maturity::Fresh,
                    skills: Vec::new(),
                    model: def.model.clone(),
                    instructions: def.instructions.clone(),
                    tools: def.tools.clone(),
                    agent_cli: "claude-code".to_string(),
                    source: source.clone(),
                    project_id: Some(project),
                })
                .await
            {
                eprintln!(
                    "[BW] sync_project_assets: create_agent「{}」失败:{e}",
                    def.name
                );
            }
        }

        if let Err(e) = self.refresh_skills().await {
            eprintln!("[BW] sync_project_assets: refresh_skills 失败:{e}");
        }
        if let Err(e) = self.refresh_agents().await {
            eprintln!("[BW] sync_project_assets: refresh_agents 失败:{e}");
        }
        self.emit(Event::SkillsChanged);
        self.emit(Event::AgentsChanged);
    }

    /// V1 Issue 1 phase2 · 工作区探活三元组:采 `evidence::collect` →
    /// 喂指标(`feed_workspace_metrics`)→ 扫 assets(`sync_project_assets`)。
    /// `CreateProject` 末尾与 `CompleteCreation` 末尾共用;失败只 `eprintln!`,
    /// 绝不阻断创建流本身(创建永不因探活失败而崩)。`label` 区分日志来源,
    /// 与原两处内联的 `eprintln!` 文案逐字一致。空工作区的守卫留在各调用
    /// 处(`CreateProject` 需判 `workspace_path` 非空;`CompleteCreation` 的
    /// `path` 刚 mint 出来,调用处不守)——差异保留,不强同化。
    pub(crate) async fn probe_workspace(
        &mut self,
        project: ProjectId,
        workspace: &str,
        label: &str,
    ) {
        match evidence::collect(workspace).await {
            Ok(ev) => {
                let _ = self.feed_workspace_metrics(project, &ev).await;
                self.sync_project_assets(project, workspace).await;
            }
            Err(e) => {
                eprintln!("[BW] {label} 工作区探活失败:{e}");
            }
        }
    }

    /// CreateProject 远端建仓/接入四臂(github New/Existing + codehub New/Existing)
    /// 共用这段:未配 `workspaces_root` → 建仓根本起不来,发同款
    /// `ConnectorSynced(Fail)` + `ActionProgress(Fail)` 对子。各臂只在
    /// provider 标签("GitHub"/"CodeHub")与"为什么"文案上不同,其余逐字
    /// 一致——抽出来收口,避免四份逐字复制。
    pub(crate) fn fail_no_workspaces_root(
        &mut self,
        action_name: &str,
        proj_name: &str,
        provider_label: &str,
        detail: &str,
    ) {
        let detail = detail.to_string();
        self.emit(Event::ConnectorSynced {
            name: format!("{} · {}", proj_name, provider_label),
            ok: false,
            detail: detail.clone(),
        });
        self.emit(Event::ActionProgress {
            name: action_name.to_string(),
            state: ActionState::Fail(detail),
        });
    }

    /// C4 · issue 身份映射(plan/13 D2): a project with a `remote_path`
    /// gets every BW-minted Issue mirrored as a real GitHub issue — the issue
    /// number is the Issue's cross-system identity. Called AFTER the Issue
    /// already exists in `bw-store`, so a `gh` failure never blocks the
    /// BW-side create (创建不破): the Issue simply keeps `github_number = 0`
    /// and an honest `ConnectorSynced { ok: false, .. }` toast fires — same
    /// soft-degrade shape as the Repo 卡片's `create_repo`/`clone_repo`
    /// paths. `remote_path` empty (no repo, or a 存量项目) short-circuits
    /// before touching `gh` at all — today's behavior, byte-for-byte.
    ///
    /// `announce` (plan/14 C14): only the creation flow's standard-Issue trio
    /// (`seed_standard_issue_trio`) opts into the pending/ok/fail
    /// `Event::ActionProgress` triple — this function's other two callers
    /// (`Command::CreateIssue`'s op-panel manual create, Autopilot's
    /// cron-fired create) are out of C14's scope (CLAUDE.md: 范围收敛,op
    /// 面板既有动作不动) and pass `false`, leaving their behavior
    /// byte-for-byte unchanged.
    pub(crate) async fn sync_issue_to_github(
        &mut self,
        project_id: ProjectId,
        issue_id: IssueId,
        title: &str,
        desc: &str,
        announce: bool,
    ) -> Result<(), AppError> {
        let proj = self
            .store
            .get_project(project_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let path = proj.remote_path.trim();
        if path.is_empty() {
            return Ok(());
        }
        let remote =
            bw_engine::remote::Remote::for_project(&proj.provider, &proj.remote_host, path)?;
        let body = if desc.trim().is_empty() {
            "(BW 建单同步,未填写详情)".to_string()
        } else {
            desc.trim().to_string()
        };
        let action_name = format!("{title} · 建单");
        if announce {
            self.emit(Event::ActionProgress {
                name: action_name.clone(),
                state: ActionState::Started,
            });
        }
        match remote.create_issue(title, &body).await {
            Ok(gh_number) => {
                self.store
                    .set_issue_github_number(issue_id, gh_number)
                    .await?;
                if announce {
                    self.emit(Event::ActionProgress {
                        name: action_name,
                        state: ActionState::Ok(format!("#{gh_number}")),
                    });
                }
            }
            Err(e) => {
                self.emit(Event::ConnectorSynced {
                    name: format!("{title} · GitHub Issue"),
                    ok: false,
                    detail: format!("GitHub 开 issue 失败,BW 侧 Issue 已建立,号未映射:{e}"),
                });
                if announce {
                    self.emit(Event::ActionProgress {
                        name: action_name,
                        state: ActionState::Fail(e.to_string()),
                    });
                }
            }
        }
        Ok(())
    }

    /// C8 · 标配 Issue 三件套(plan/13 D8): 创建流落地(`CompleteCreation`)
    /// 时,挂仓项目自动建三张标配 Issue——竞品分析→找指标→绑数据,依赖序
    /// 即建单序即编号序(`create_issue` 按项目内 `MAX(number)+1` 分配,这里
    /// 是这个新项目的头三张 Issue,天然拿到 1/2/3)。每张都走既有
    /// `sync_issue_to_github` 真开一个 GitHub issue——`remote_path` 为空
    /// 的项目在那里短路返回,这里的 BW 侧建号仍然发生,只是 `github_number`
    /// 留 0(和手动建单同一诚实口径),但本函数在调用前就已经用
    /// `remote_path` 是否非空短路整批——无仓项目连 BW 侧的三张都不建,
    /// 不给建不了仓的项目发一套没处交付的活(如实留白)。
    ///
    /// 每张携带一个稳定 `standard_skill` slug——三张卡均已由 Boot 的
    /// `seed_bw_standard_skills_if_missing` 按名种下(C9 落地
    /// `north-star-discovery` / `metrics-binding`,C10 补上
    /// `competitive-analysis`,plan/17 起统一走包文档解析播种),
    /// `run_issue_now` 注入时按名查到即真实注入。
    ///
    /// 返回①竞品分析那张的 `IssueId`,供「问一句就跑」路径直接开工;无仓
    /// 项目(未建任何标配票)返回 `None`。
    pub(crate) async fn seed_standard_issue_trio(
        &mut self,
        project: ProjectId,
    ) -> Result<Option<IssueId>, AppError> {
        let proj = self
            .store
            .get_project(project)
            .await?
            .ok_or(AppError::NotFound)?;
        if proj.remote_path.trim().is_empty() {
            return Ok(None);
        }
        // V2-② Phase A (§6.2/§5.2): later-comer gate — if the repo already
        // has `.bw/project.toml` (another Buddy纳管过这个仓), skip the trio.
        // The old gate (remote_path non-empty) fired for every repo-attached
        // project, so a second Buddy adopting the same repo would re-create
        // 3 duplicate Issues on the remote. The file's existence is the
        // first-comer/later-comer判据 (§6): no file = first-comer (build the
        // trio); file = later-comer (skip — the startup package was already
        // issued by the first Buddy).
        let project_toml = std::path::Path::new(&proj.workspace_path)
            .join(bw_engine::project_file::PROJECT_FILE_REL_PATH);
        if project_toml.exists() {
            return Ok(None);
        }
        const TRIO: [(&str, &str, &str); 3] = [
            (
                "竞品分析",
                "起草对标名单、各家北极星猜测、差异定位、可借鉴打法,产出报告 PR 进仓\
                 (docs/competitive-analysis.md)。是「找指标」那张的输入。执行器联网检索\
                 不通时如实降级为「人喂材料 + agent 整理」,报告绝不由幻觉填充。",
                "competitive-analysis",
            ),
            (
                "找指标",
                "结合项目意图与「竞品分析」那张产出的 docs/competitive-analysis.md,\
                 推导北极星 + 滞后 + 引领三层指标,每条附采集方案——先对后亮,北极星绝不为\
                 「采得到」退化成工程虚荣指标。挂 Skill: north-star-discovery。",
                "north-star-discovery",
            ),
            (
                "绑数据",
                "为「找指标」草拟的、绑不上数据的指标(.bw/metrics.toml)找到点亮的最便宜\
                 路径——绝不伪造数据、绝不为了点亮而改指标定义。挂 Skill: metrics-binding。",
                "metrics-binding",
            ),
        ];
        let mut first: Option<IssueId> = None;
        for (title, desc, skill_slug) in TRIO {
            let id = IssueId::new();
            self.store
                .create_issue(NewIssue {
                    id,
                    project_id: project,
                    stage: StageKind::Prototype,
                    title: title.to_string(),
                    desc: desc.to_string(),
                    priority: IssuePriority::Medium,
                    standard_skill: skill_slug.to_string(),
                })
                .await?;
            // announce=true (plan/14 C14): this is the creation-flow trio —
            // the one `sync_issue_to_github` call site that should surface a
            // pending/ok/fail `ActionProgress` triple.
            self.sync_issue_to_github(project, id, title, desc, true)
                .await?;
            if first.is_none() {
                first = Some(id);
            }
        }
        Ok(first)
    }

    /// V1 Issue2 Phase 3 · §3.2: sync `.bw/connectors.toml` → SQLite
    /// `connector` rows. Parallel to [`sync_metrics_file_for`] — the file is
    /// the source of truth, buddy DB is a mirror. merge after `MergeIssuePr`
    /// auto-calls this (alongside `sync_metrics_file_for`). File not
    /// existing = zero action zero noise (same idiom). Bad file = honest
    /// error toast, cache untouched.
    ///
    /// Only `kind = "script"` connectors are synced from the file (other
    /// kinds live in the DB from their creation paths). Each connector is
    /// upserted by `(project_id, name)` — existing rows keep their id, new
    /// rows get `kind = 'script'` + default operational fields.
    pub(crate) async fn sync_connectors_file_for(&mut self, p: ProjectId) -> Result<(), AppError> {
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
        match bw_engine::connectors_file::read(&proj.workspace_path) {
            Ok(None) => {}
            Ok(Some(file)) => {
                let sync = connectors_file_sync(p, &file);
                let summary = self.store.sync_connectors_file(sync).await?;
                // File→DB upsert alone is not enough for Hub/rail: UI reads
                // `state.connectors`. Without refresh, a newly synced script
                // connector (e.g. version-release after 绑数据 merge) stays
                // invisible until Boot or an unrelated ConnectorsChanged path.
                self.refresh_connectors().await?;
                self.emit(Event::ConnectorsChanged);
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ConnectorSynced {
                    name: "connectors.toml".into(),
                    ok: true,
                    detail: format!("{} 个 script 连接器已同步", summary.connectors_synced),
                });
            }
            Err(e) => {
                self.emit(Event::ConnectorSynced {
                    name: "connectors.toml".into(),
                    ok: false,
                    detail: e.to_string(),
                });
            }
        }
        Ok(())
    }

    /// V2-② Phase A (§6): sync `.bw/project.toml` → SQLite `project` row
    /// (`name`/`kind`/`descr`/`benchmark`/`opportunity`). Parallels
    /// [`sync_metrics_file_for`] — the file is the source of truth, buddy DB
    /// is a mirror. File missing = zero action zero noise (same idiom). Bad
    /// file = honest error toast, cache untouched. Never triggers recompute
    /// (intent fields aren't derived data). Called by the creation flow's
    /// later-comer detection path and `Command::SyncProjectFile`.
    pub(crate) async fn sync_project_file_for(&mut self, p: ProjectId) -> Result<(), AppError> {
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
        match bw_engine::project_file::read(&proj.workspace_path) {
            Ok(None) => {}
            Ok(Some(file)) => {
                let sync = project_file_sync(p, &file);
                self.store.sync_project_file(sync).await?;
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ConnectorSynced {
                    name: "project.toml".into(),
                    ok: true,
                    detail: "仓里 .bw/project.toml 正本已读回,本地意图字段已对齐正本".into(),
                });
            }
            Err(e) => {
                self.emit(Event::ConnectorSynced {
                    name: "project.toml".into(),
                    ok: false,
                    detail: e.to_string(),
                });
            }
        }
        Ok(())
    }

    /// V2-②-I: exact title → standard trio skill slug. Empty = no default skill.
    pub(crate) fn trio_skill_for_title(title: &str) -> &'static str {
        match title.trim() {
            "竞品分析" => "competitive-analysis",
            "找指标" => "north-star-discovery",
            "绑数据" => "metrics-binding",
            _ => "",
        }
    }

    /// V2-②-I: import/refresh local issue rows from remote open issues.
    ///
    /// - Never calls remote `create_issue`
    /// - Never transitions to Done
    /// - Same `github_number` → refresh title/desc; fill empty skill only
    /// - New numbers → create local Backlog rows
    /// - Local mapped row absent from open set + not Done/settled → Cancelled
    ///   (board already hides Cancelled); local Done kept for resume
    ///
    /// `announce`: emit ActionProgress Started/Ok/Fail (manual + creation).
    pub(crate) async fn sync_remote_issues_for(
        &mut self,
        p: ProjectId,
        announce: bool,
    ) -> Result<(), AppError> {
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
        let path = proj.remote_path.trim();
        if path.is_empty() {
            return Ok(());
        }
        let action_name = format!("{} · 从仓同步 Issue", proj.name);
        if announce {
            self.emit(Event::ActionProgress {
                name: action_name.clone(),
                state: ActionState::Started,
            });
        }
        let remote =
            match bw_engine::remote::Remote::for_project(&proj.provider, &proj.remote_host, path) {
                Ok(r) => r,
                Err(e) => {
                    if announce {
                        self.emit(Event::ActionProgress {
                            name: action_name,
                            state: ActionState::Fail(e.to_string()),
                        });
                    }
                    self.emit(Event::ConnectorSynced {
                        name: "远端 Issue".into(),
                        ok: false,
                        detail: format!("远端配置错,无法读回 Issue:{e}"),
                    });
                    return Ok(());
                }
            };
        let opens = match remote.list_open_issues().await {
            Ok(v) => v,
            Err(e) => {
                if announce {
                    self.emit(Event::ActionProgress {
                        name: action_name,
                        state: ActionState::Fail(e.to_string()),
                    });
                }
                self.emit(Event::ConnectorSynced {
                    name: "远端 Issue".into(),
                    ok: false,
                    detail: format!("列出远端 open Issue 失败:{e}"),
                });
                return Ok(());
            }
        };
        let (created, refreshed, pruned) = self
            .import_remote_open_issues(p, proj.active_stage, &opens)
            .await?;
        self.refresh_issues().await?;
        self.emit(Event::IssuesChanged);
        let detail = format!(
            "新建 {created} · 刷新 {refreshed} · 收起 {pruned} · 远端 open {}",
            opens.len()
        );
        if announce {
            self.emit(Event::ActionProgress {
                name: action_name,
                state: ActionState::Ok(detail.clone()),
            });
        }
        self.emit(Event::ConnectorSynced {
            name: "远端 Issue".into(),
            ok: true,
            detail,
        });
        Ok(())
    }

    /// Pure store-side apply for V2-②-I (unit-testable without network).
    /// Returns `(created, refreshed, pruned)` where `pruned` is how many
    /// unsettled local rows were Cancelled because their remote left open.
    pub(crate) async fn import_remote_open_issues(
        &mut self,
        p: ProjectId,
        stage: StageKind,
        opens: &[bw_engine::github::RemoteOpenIssue],
    ) -> Result<(u32, u32, u32), AppError> {
        let existing = self.store.list_issues(p, None, None).await?;
        let mut by_number: std::collections::HashMap<u32, IssueId> = existing
            .into_iter()
            .filter(|i| i.github_number != 0)
            .map(|i| (i.github_number, i.id))
            .collect();
        let mut created = 0u32;
        let mut refreshed = 0u32;
        let mut open_set = std::collections::HashSet::new();
        for remote in opens {
            if remote.number == 0 {
                continue;
            }
            open_set.insert(remote.number);
            let skill = Self::trio_skill_for_title(&remote.title);
            let body = remote.body.trim();
            if let Some(id) = by_number.get(&remote.number).copied() {
                self.store
                    .update_issue_content(id, &remote.title, body)
                    .await?;
                self.store
                    .set_issue_standard_skill_if_empty(id, skill)
                    .await?;
                refreshed += 1;
            } else {
                let id = IssueId::new();
                self.store
                    .create_issue(NewIssue {
                        id,
                        project_id: p,
                        stage,
                        title: remote.title.clone(),
                        desc: body.to_string(),
                        priority: IssuePriority::Medium,
                        standard_skill: skill.to_string(),
                    })
                    .await?;
                self.store
                    .set_issue_github_number(id, remote.number)
                    .await?;
                by_number.insert(remote.number, id);
                created += 1;
            }
        }

        // Remote closed (absent from open list): drop unsettled local rows
        // off the board via Cancelled. Keep Done/settled — this Buddy's own
        // completion ledger + resume chat. Local-only rows (github_number=0)
        // are untouched.
        let mut pruned = 0u32;
        let after = self.store.list_issues(p, None, None).await?;
        for issue in after {
            if issue.github_number == 0 || open_set.contains(&issue.github_number) {
                continue;
            }
            if issue.status == IssueStatus::Done || issue.settled_at.is_some() {
                continue;
            }
            if issue.status == IssueStatus::Cancelled {
                continue;
            }
            if !issue.status.can_transition_to(IssueStatus::Cancelled) {
                continue;
            }
            self.store
                .transition_issue(issue.id, IssueStatus::Cancelled)
                .await?;
            // If this issue held the delivery lock, release it (Cancelled via
            // TransitionIssue also skips demote; sync must not leave a lock).
            let _ = self.demote_delivery_to_consultation(issue.id).await;
            pruned += 1;
        }
        Ok((created, refreshed, pruned))
    }

    /// V2-② Phase A (§7): existing-repo first-comer path — write
    /// `.bw/project.toml` into the cloned workspace, open a PR on
    /// `bw/project-init`, and Buddy auto-merge it. project.toml is
    /// configuration (not an Issue), so auto-merge here doesn't break
    /// "Done 永不自动" (that rule is about Issues; issue PRs are never
    /// auto-merged — unchanged). On any failure (no merge permission, branch
    /// protection, network), the file is still written to the workspace and
    /// the PR may be open — a tip toast surfaces the error, Builder handles
    /// it manually. Never blocks CompleteCreation itself.
    pub(crate) async fn write_project_toml_pr(
        &mut self,
        proj: &ProjectRow,
        dir: &std::path::Path,
        toml: &str,
    ) {
        let action_name = format!("{} · project.toml PR", proj.name);
        self.emit(Event::ActionProgress {
            name: action_name.clone(),
            state: ActionState::Started,
        });
        // Write the file (no commit — the PR path stages + commits via
        // stage_commit_push_msg). git checkout -b carries untracked files to
        // the new branch, so the written file is staged + committed there.
        let toml_path = dir.join(bw_engine::project_file::PROJECT_FILE_REL_PATH);
        if let Some(parent) = toml_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&toml_path, toml) {
            self.emit(Event::ActionProgress {
                name: action_name,
                state: ActionState::Fail(format!("写入 .bw/project.toml 失败:{e}")),
            });
            return;
        }
        let remote = match bw_engine::remote::Remote::for_project(
            &proj.provider,
            &proj.remote_host,
            &proj.remote_path,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.emit(Event::ActionProgress {
                    name: action_name,
                    state: ActionState::Fail(format!("远端配置错,无法提 PR:{e}")),
                });
                return;
            }
        };
        match remote.create_project_init_mr(dir, "项目意图正本").await {
            Ok(pr_opened) => {
                let pr_num = pr_opened.number();
                match remote.merge_mr(pr_num).await {
                    Ok(()) => {
                        self.emit(Event::ActionProgress {
                            name: action_name,
                            state: ActionState::Ok(format!("PR #{pr_num} 已合入")),
                        });
                    }
                    Err(e) => {
                        // PR is open but auto-merge failed — honest tip,
                        // Builder can manually merge. The file is written +
                        // committed on the branch.
                        let detail = format!(
                            "project.toml PR 已开(#{pr_num})但自动合入失败,请手动 merge:{e}"
                        );
                        self.emit(Event::ConnectorSynced {
                            name: format!("{} · project.toml", proj.name),
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
            Err(e) => {
                let detail = format!("project.toml 提 PR 失败(文件已写入工作区,可手动提交):{e}");
                self.emit(Event::ConnectorSynced {
                    name: format!("{} · project.toml", proj.name),
                    ok: false,
                    detail: detail.clone(),
                });
                self.emit(Event::ActionProgress {
                    name: action_name,
                    state: ActionState::Fail(detail),
                });
            }
        }
        // `create_project_init_mr` checks out `bw/project-init` and leaves
        // HEAD there. Subsequent issue worktrees branch from current HEAD —
        // without returning to the default branch, first-comer issue PRs can
        // fork off the config branch. Best-effort: always try to sync back
        // (merge Ok → pull project.toml; merge/PR fail → at least leave main).
        if let Err(e) = bw_engine::github::sync_default_branch(dir).await {
            self.emit(Event::ConnectorSynced {
                name: format!("{} · project.toml", proj.name),
                ok: false,
                detail: format!(
                    "project.toml 流程后工作区收拢默认分支失败(可能仍停在 bw/project-init,后续开活前请手动切回主干):{e}"
                ),
            });
        }
    }

    /// Scan `workspace` (real `git ls-files` + `stat` + short HEAD) and
    /// register every tracked file as an artifact version. Idempotent at the
    /// store layer — returns only the genuinely-new count.
    pub(crate) async fn scan_and_register_artifacts(
        &self,
        project: ProjectId,
        workspace: &str,
        workflow_run_id: Option<WorkflowRunId>,
        stage_kind: Option<StageKind>,
        issue_id: Option<IssueId>,
    ) -> Result<u32, AppError> {
        let files = evidence::list_workspace_files(workspace)
            .await
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        if files.is_empty() {
            return Ok(0);
        }
        let commit = evidence::head_commit(workspace)
            .await
            .map_err(|e| AppError::Invalid(e.to_string()))?
            .unwrap_or_default();
        let registered_at = now().unix_timestamp();
        let items = files
            .into_iter()
            .map(|f| NewArtifact {
                id: ArtifactId::new(),
                project_id: project,
                workflow_run_id,
                issue_id,
                stage_kind,
                kind: classify_artifact_path(&f.path),
                path: f.path,
                bytes: f.bytes,
                git_commit: commit.clone(),
                registered_at,
            })
            .collect();
        Ok(self.store.register_artifacts(items).await?)
    }
}

/// V2-②-I: remote open → local Backlog; idempotent on github_number.
#[cfg(test)]
mod sync_remote_issues_tests {
    use super::*;
    use bw_core::model::{IssueStatus, MaturityPeriod};
    use bw_store::SqliteStore;

    async fn boot_project(db: &str) -> (App, Arc<dyn Store>, ProjectId) {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(db).await.expect("open db"));
        let mut app = App::new(store.clone(), ClaudeCliConfig::default());
        app.dispatch(Command::Boot).await.expect("boot");
        let pid = ProjectId::new();
        app.dispatch(Command::CreateProject {
            provider: "github".into(),
            id: pid,
            name: "sync-remote-issues".into(),
            kind: "test".into(),
            desc: "V2-②-I".into(),
            workspace: None,
            github: None,
            codehub: None,
        })
        .await
        .expect("create");
        app.dispatch(Command::SetCycle {
            cycle: MaturityPeriod::Explore,
        })
        .await
        .expect("cycle");
        app.dispatch(Command::CompleteCreation {
            cadence: Cadence::Weekly,
            run_first: false,
        })
        .await
        .expect("complete");
        app.dispatch(Command::OpenProject(pid)).await.expect("open");
        (app, store, pid)
    }

    #[test]
    fn trio_skill_exact_title_only() {
        assert_eq!(App::trio_skill_for_title("找指标"), "north-star-discovery");
        assert_eq!(App::trio_skill_for_title(" 绑数据 "), "metrics-binding");
        assert_eq!(
            App::trio_skill_for_title("竞品分析"),
            "competitive-analysis"
        );
        assert_eq!(App::trio_skill_for_title("找指标（重做）"), "");
        assert_eq!(App::trio_skill_for_title("手工活"), "");
    }

    #[tokio::test]
    async fn import_creates_backlog_and_is_idempotent() {
        let db =
            std::env::temp_dir().join(format!("bw_sync_remote_issues_{}.db", uuid::Uuid::new_v4()));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;

        let opens = vec![
            bw_engine::github::RemoteOpenIssue {
                number: 11,
                title: "找指标".into(),
                body: "挂 Skill: north-star-discovery".into(),
            },
            bw_engine::github::RemoteOpenIssue {
                number: 12,
                title: "网页随手开的".into(),
                body: "out of band".into(),
            },
        ];
        let (c1, r1, p1) = app
            .import_remote_open_issues(pid, StageKind::Prototype, &opens)
            .await
            .expect("import1");
        assert_eq!((c1, r1, p1), (2, 0, 0));

        let issues = store.list_issues(pid, None, None).await.expect("list");
        assert_eq!(issues.len(), 2);
        let find = |n: u32| issues.iter().find(|i| i.github_number == n).unwrap();
        let a = find(11);
        assert_eq!(a.title, "找指标");
        assert_eq!(a.status, IssueStatus::Backlog);
        assert_eq!(a.standard_skill, "north-star-discovery");
        let b = find(12);
        assert_eq!(b.standard_skill, "");
        assert_eq!(b.status, IssueStatus::Backlog);

        // Second pass: refresh #11 only; #12 left open-set → Cancelled (off-board).
        let opens2 = vec![bw_engine::github::RemoteOpenIssue {
            number: 11,
            title: "找指标".into(),
            body: "updated body".into(),
        }];
        let (c2, r2, p2) = app
            .import_remote_open_issues(pid, StageKind::Prototype, &opens2)
            .await
            .expect("import2");
        assert_eq!((c2, r2, p2), (0, 1, 1));
        let again = store.get_issue(a.id).await.expect("get").expect("row");
        assert_eq!(again.desc, "updated body");
        assert_eq!(again.status, IssueStatus::Backlog);
        let closed_elsewhere = store.get_issue(b.id).await.expect("get12").expect("row");
        assert_eq!(
            closed_elsewhere.status,
            IssueStatus::Cancelled,
            "remote-closed unsettled → Cancelled"
        );
        assert_eq!(
            store.list_issues(pid, None, None).await.unwrap().len(),
            2,
            "no duplicate local rows"
        );

        // Empty skill can be filled on refresh; non-empty never overwritten.
        let id12 = b.id;
        store
            .set_issue_standard_skill_if_empty(id12, "should-not-apply-when-we-set-first")
            .await
            .unwrap();
        // Simulate user already having a skill — write via empty-gate twice:
        // first fill wins.
        let issues = store.list_issues(pid, None, None).await.unwrap();
        let b2 = issues.iter().find(|i| i.id == id12).unwrap();
        assert_eq!(b2.standard_skill, "should-not-apply-when-we-set-first");
        store
            .set_issue_standard_skill_if_empty(id12, "north-star-discovery")
            .await
            .unwrap();
        let b3 = store.get_issue(id12).await.unwrap().unwrap();
        assert_eq!(b3.standard_skill, "should-not-apply-when-we-set-first");

        let _ = std::fs::remove_file(&db);
    }

    #[tokio::test]
    async fn remote_closed_keeps_local_done_cancels_unsettled() {
        let db =
            std::env::temp_dir().join(format!("bw_sync_remote_prune_{}.db", uuid::Uuid::new_v4()));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;

        let opens = vec![
            bw_engine::github::RemoteOpenIssue {
                number: 21,
                title: "我做完的".into(),
                body: "".into(),
            },
            bw_engine::github::RemoteOpenIssue {
                number: 22,
                title: "别人关的".into(),
                body: "".into(),
            },
            bw_engine::github::RemoteOpenIssue {
                number: 23,
                title: "我开跑中".into(),
                body: "".into(),
            },
        ];
        let (c, r, p) = app
            .import_remote_open_issues(pid, StageKind::Prototype, &opens)
            .await
            .expect("seed");
        assert_eq!((c, r, p), (3, 0, 0));

        let issues = store.list_issues(pid, None, None).await.unwrap();
        let mine = issues.iter().find(|i| i.github_number == 21).unwrap();
        let theirs = issues.iter().find(|i| i.github_number == 22).unwrap();
        let mine_wip = issues.iter().find(|i| i.github_number == 23).unwrap();

        // Simulate this Buddy completing #21 through the real Done edge.
        store
            .transition_issue(mine.id, IssueStatus::Todo)
            .await
            .unwrap();
        store
            .transition_issue(mine.id, IssueStatus::InProgress)
            .await
            .unwrap();
        store
            .transition_issue(mine.id, IssueStatus::InReview)
            .await
            .unwrap();
        app.dispatch(Command::TransitionIssue {
            id: mine.id,
            status: IssueStatus::Done,
        })
        .await
        .expect("done");
        let done_row = store.get_issue(mine.id).await.unwrap().unwrap();
        assert_eq!(done_row.status, IssueStatus::Done);
        assert!(done_row.settled_at.is_some());

        store
            .transition_issue(mine_wip.id, IssueStatus::Todo)
            .await
            .unwrap();
        store
            .transition_issue(mine_wip.id, IssueStatus::InProgress)
            .await
            .unwrap();

        // Both remotes closed; open set empty → keep Done, cancel the rest.
        let (c2, r2, p2) = app
            .import_remote_open_issues(pid, StageKind::Prototype, &[])
            .await
            .expect("prune");
        assert_eq!((c2, r2, p2), (0, 0, 2));

        let kept = store.get_issue(mine.id).await.unwrap().unwrap();
        assert_eq!(kept.status, IssueStatus::Done);
        assert!(kept.settled_at.is_some());

        let dropped = store.get_issue(theirs.id).await.unwrap().unwrap();
        assert_eq!(dropped.status, IssueStatus::Cancelled);

        let wip_dropped = store.get_issue(mine_wip.id).await.unwrap().unwrap();
        assert_eq!(wip_dropped.status, IssueStatus::Cancelled);

        let _ = std::fs::remove_file(&db);
    }
}
