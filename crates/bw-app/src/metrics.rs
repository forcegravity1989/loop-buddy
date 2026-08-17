//! 指标:阶段完成计数播种、工作区指标回流、`.bw/metrics.toml` 同步、按连接器采集。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// A4: name of the machine-fed "完成 Issue 数" leading metric, seeded per
    /// project×stage with an EMPTY target so its signal stays Unknown (honest
    /// "no goal set") — never a fake green from a raw completion count.
    pub(crate) fn stage_done_metric_name() -> &'static str {
        "阶段完成 Issue 数"
    }

    /// A4: idempotently seed the per-stage "完成 Issue 数" leading metric (one
    /// per stage, empty target). By-name idempotent — a re-seed adds nothing,
    /// so Boot can backfill pre-A4 projects safely.
    pub(crate) async fn seed_stage_done_metrics(&self, project: ProjectId) -> Result<(), AppError> {
        let have: std::collections::HashSet<StageKind> = self
            .store
            .persisted_signals(project)
            .await?
            .metrics
            .into_iter()
            .filter(|m| m.name == Self::stage_done_metric_name())
            .filter_map(|m| m.stage_kind)
            .collect();
        for kind in StageKind::ALL {
            if have.contains(&kind) {
                continue;
            }
            self.store
                .upsert_metric(NewMetric {
                    id: MetricId::new(),
                    project_id: project,
                    role: MetricRole::Leading,
                    stage_kind: Some(kind),
                    name: Self::stage_done_metric_name().into(),
                    def: "本阶段已完成的 Issue 数(每次 Done 自动计数,机器源)".into(),
                    target_raw: String::new(),
                    amber: AmberBand::default(),
                    last_target: String::new(),
                    driver: String::new(),
                    pos: 100 + kind.index() as i64,
                    collect_kind: String::new(),
                    collect_query: String::new(),
                })
                .await?;
        }
        Ok(())
    }

    /// P5 · codehub 公共指标 seed(对标 `seed_stage_done_metrics` 的幂等套路):
    /// 给 `provider=codehub` 且 `remote_path` 非空的项目种两条默认公共指标
    /// ——开放 Issue 数 + 已合入 MR 数。V1 Issue 1 phase2:`collect_kind`
    /// 从 `'codehub'`(inline arm)改为 `'script'`(script connector arm),
    /// `collect_query` 从 `'issues:opened'`/`'mrs:merged'` 改为
    /// `'open_issues'`/`'merged_mrs'`(script 输出 JSON 的字段路径)。
    /// cron → `collect_project_metrics` → script arm 预跑「codehub 仓统计」
    /// connector → 输出 JSON → metric 按 `collect_query` 取值 → observation。
    /// **不用定义**——V3 规划里「公共指标默认有」的落地。非 codehub / 没
    /// 接远端 → no-op(留 github 的既有口径不动)。inline codehub arm 留兼容
    /// (§6 偏差:本 issue 不动 collect_project_metrics 的 inline arm)。
    pub(crate) async fn seed_codehub_public_metrics(
        &self,
        project: ProjectId,
    ) -> Result<(), AppError> {
        let proj = self
            .store
            .get_project(project)
            .await?
            .ok_or(AppError::NotFound)?;
        if proj.provider != "codehub" || proj.remote_path.trim().is_empty() {
            return Ok(());
        }
        // by-name 幂等:已有同名不重种(不抹既有值)。
        let have: std::collections::HashSet<String> = self
            .store
            .persisted_signals(project)
            .await?
            .metrics
            .into_iter()
            .map(|m| m.name)
            .collect();
        // (name, def, query) —— 公共计数,滞后指标(Lagging),空 target 保持
        // signal Unknown(计数不是目标,只为点亮「有数据」)。
        // V1 Issue 1: query 改为 script 输出 JSON 的字段路径(open_issues /
        // merged_mrs),collect_kind 改为 script。
        const PAIR: [(&str, &str, &str); 2] = [
            (
                "开放 Issue 数",
                "codehub 仓当前 state=opened 的 issue 计数(机器源,buddy 自带 script connector 跑 codehub-cli issue list --jq length)",
                "open_issues",
            ),
            (
                "已合入 MR 数",
                "codehub 仓累计 state=merged 的 MR 计数(机器源,buddy 自带 script connector 跑 codehub-cli mr list --jq length)",
                "merged_mrs",
            ),
        ];
        for (idx, (name, def, query)) in PAIR.iter().copied().enumerate() {
            if have.contains(name) {
                continue;
            }
            self.store
                .upsert_metric(NewMetric {
                    id: MetricId::new(),
                    project_id: project,
                    role: MetricRole::Lagging,
                    stage_kind: None,
                    name: name.to_string(),
                    def: def.to_string(),
                    target_raw: String::new(),
                    amber: AmberBand::default(),
                    last_target: String::new(),
                    driver: String::new(),
                    pos: 200 + idx as i64,
                    collect_kind: "script".into(),
                    collect_query: query.to_string(),
                })
                .await?;
        }
        Ok(())
    }

    /// A4: feed the stage's "完成 Issue 数" metric the current count of Done
    /// issues in that stage, but only when it changed (change-guard — same
    /// idempotency as a manual re-confirm). Machine source (Telemetry). The
    /// metric's empty target keeps its signal Unknown: a count is not a goal.
    pub(crate) async fn feed_stage_done_count(
        &self,
        project: ProjectId,
        stage: StageKind,
    ) -> Result<(), AppError> {
        self.seed_stage_done_metrics(project).await?;
        let done = self
            .store
            .list_issues(project, Some(stage), Some(IssueStatus::Done))
            .await?
            .len() as i64;
        let new_raw = done.to_string();
        let metric = self
            .store
            .persisted_signals(project)
            .await?
            .metrics
            .into_iter()
            .find(|m| {
                m.name == Self::stage_done_metric_name()
                    && m.stage_kind == Some(stage)
                    // 停用的指标不再进新观测:停用就是"别再拿这条量我了"。
                    && !m.archived
            });
        let Some(m) = metric else {
            return Ok(()); // metric missing/archived — honest no-op
        };
        if m.value_raw == new_raw {
            return Ok(()); // change-guard: no new fact
        }
        self.store
            .append_observation(m.id, SourceKind::Telemetry, &new_raw, now())
            .await?;
        self.store.recompute_signals(project, now()).await?;
        Ok(())
    }

    /// Feed a real workspace evidence reading into the project's matching
    /// metrics (`METRIC_WS_COMMITS` / `METRIC_WS_DOCS`) as
    /// `SourceKind::Connector` observations. Metrics the project hasn't
    /// defined are skipped — the kernel never invents a metric (targets are
    /// human intent, not machine output).
    pub(crate) async fn feed_workspace_metrics(
        &mut self,
        project: ProjectId,
        ev: &evidence::WorkspaceEvidence,
    ) -> Result<(), AppError> {
        let sigs = self.store.persisted_signals(project).await?;
        let mut touched = false;
        for (name, value) in [
            (METRIC_WS_COMMITS, ev.commit_count.to_string()),
            (METRIC_WS_DOCS, ev.docs_files.to_string()),
        ] {
            // 停用的指标不再进新观测(同 feed_stage_done_count)。
            let Some(m) = sigs.metrics.iter().find(|m| m.name == name && !m.archived) else {
                continue;
            };
            if m.value_raw == value {
                continue; // unchanged — a re-probe is not a new fact
            }
            self.store
                .append_observation(m.id, SourceKind::Connector, &value, now())
                .await?;
            touched = true;
        }
        if touched {
            self.store.recompute_signals(project, now()).await?;
            self.emit(Event::ProjectUpdated(project));
        }
        Ok(())
    }

    /// C7 · 采集器 (plan/13 D7): pull real data into `project`'s metrics as
    /// append-only observations. Shared by the manual `Command::CollectMetrics`
    /// and the standard collect cron (`tick_scheduler`), so both entrances walk
    /// exactly one code path.
    ///
    /// v1 只真采一类:每条 `collect.kind = "github"` 指标跑一次真实 `gh` 计数
    /// 查询,change-guard 命中(值未变)则不重复记点;值变了才 append 一个观测
    /// 并 `recompute_signals`(唯一派生入口)。`bw`/`connector` 两类**如实留白**
    /// ——不采集、不写零值,signal 保持既有(无数据即 Unknown,绝不假绿)。
    /// `gh` 失败:零新观测、如实计入失败(供 ok:false toast),signal 靠既有
    /// 过期降级自然变灰。北极星的采集方案落在 `project` 列(非 metric 行),没有
    /// 可挂观测的 metric_id,v1 不采(留白见 docs/buddy/standards/metrics.md)。
    /// `.bw/metrics.toml` 正本 → SQLite 缓存的同步本体。两个入口共用:
    /// `Command::SyncMetricsFile`(手动,active 项目)与 `MergeIssuePr`
    /// (plan/13 D5「merge 后同步进 SQLite 作缓存」——code-review Spec 轴
    /// 指出此前管线断在入口,全仓没有任何触发点)。
    pub(crate) async fn sync_metrics_file_for(&mut self, p: ProjectId) -> Result<(), AppError> {
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
        match bw_engine::metrics_file::read(&proj.workspace_path) {
            // No configured workspace, or a workspace with no file
            // yet — 文件不存在:零动作零噪音,行为与今日完全一致
            // (no store write, no event; `read` folds both cases
            // into the same honest `Ok(None)`).
            Ok(None) => {}
            Ok(Some(file)) => {
                let sync = metrics_file_sync(p, &file);
                let summary = self.store.sync_metrics_file(sync).await?;
                self.emit(Event::ProjectUpdated(p));
                // 自动停用只在真发生时出声(0 条不提)——正本里删掉一条
                // 指标是"界面上少了一块"这种看得见的后果,不能静默发生。
                let mut detail = format!(
                    "北极星 · {} 条滞后指标 · {} 条引领指标已同步",
                    summary.lagging_synced, summary.leading_synced
                );
                if summary.auto_archived > 0 {
                    detail.push_str(&format!(
                        ";正本里已删除的 {} 条自动停用(历史观测保留,可在「已停用」里恢复)",
                        summary.auto_archived
                    ));
                }
                self.emit(Event::ConnectorSynced {
                    name: "metrics.toml".into(),
                    ok: true,
                    detail,
                });
            }
            Err(e) => {
                // 坏 toml:如实报错,沿用 ConnectorSynced ok:false 惯例;
                // `read` only returns `Err` on a parse/IO failure — the
                // cache is untouched (nothing was written above).
                self.emit(Event::ConnectorSynced {
                    name: "metrics.toml".into(),
                    ok: false,
                    detail: e.to_string(),
                });
            }
        }
        Ok(())
    }

    pub(crate) async fn collect_project_metrics(
        &mut self,
        project: ProjectId,
    ) -> Result<MetricCollectSummary, AppError> {
        let proj = self
            .store
            .get_project(project)
            .await?
            .ok_or(AppError::NotFound)?;
        let sigs = self.store.persisted_signals(project).await?;
        let today = now().date();
        let mut summary = MetricCollectSummary::default();
        let mut touched = false;

        // V2-② Phase B: refresh Buddy-owned `.bw/collect_stats.{sh,py}` so
        // already-onboarded workspaces pick up the 30-day history emitter
        // without re-creating the project. Idempotent overwrite of buddy
        // files only — never touches project business scripts.
        if !proj.workspace_path.trim().is_empty() && !proj.remote_path.trim().is_empty() {
            write_buddy_collect_stats(&proj);
        }

        // plan18-③ · 预跑项目的 `script` connector,把脚本产出 JSON 缓存进
        // `script_outputs`,供下面 `script` kind 指标按字段路径取值。一个项目
        // 可有多个 script connector;每个脚本只跑一次(不是每指标跑一次)。
        // 脚本自身依赖(Playwright/SSO/Chrome)由项目侧保证,buddy 只 shell-out。
        let mut script_outputs: Vec<serde_json::Value> = Vec::new();
        if !proj.workspace_path.trim().is_empty()
            && sigs
                .metrics
                .iter()
                .any(|m| m.collect_kind == "script" && !m.archived)
        {
            let connectors = self.store.list_connectors().await?;
            for c in connectors.iter().filter(|c| {
                c.kind.as_str() == CONNECTOR_KIND_SCRIPT && c.project_id == Some(project)
            }) {
                let cfg: ScriptConnectorConfig =
                    match ScriptConnectorConfig::from_config(c.config.trim()) {
                        Ok(v) => v,
                        Err(e) => {
                            summary.failed += 1;
                            if summary.first_error.is_none() {
                                summary.first_error =
                                    Some(format!("script 连接器 config 解析失败:{e}"));
                            }
                            continue;
                        }
                    };
                if cfg.script.trim().is_empty() {
                    continue;
                }
                let command = if cfg.command.trim().is_empty() {
                    "python".to_string()
                } else {
                    cfg.command.trim().to_string()
                };
                if Path::new(&cfg.script).is_absolute() {
                    summary.failed += 1;
                    if summary.first_error.is_none() {
                        summary.first_error = Some(format!(
                            "script {} config 用绝对路径,需相对工作区",
                            cfg.script
                        ));
                    }
                    continue;
                }
                let script_path = Path::new(&proj.workspace_path).join(&cfg.script);
                // PF1-R5c · Windows: app 进程 PATH 可能缺 Git bin/usr-bin
                // (sh.exe/bash.exe 所在),`Command::new("sh")` 报 program not
                // found。用候选链(BW_SH_BIN env → 裸名 PATH 搜 → 从 git.exe
                // 推导全路径 → 常见安装位),NotFound 才换下一个;非零退出/超时用当前。
                let candidates = script_interpreter_candidates(&command);
                let mut out: Option<std::process::Output> = None;
                for cand in &candidates {
                    let run = tokio::process::Command::new(cand)
                        .arg(&script_path)
                        .current_dir(&proj.workspace_path)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .output();
                    match tokio::time::timeout(std::time::Duration::from_secs(300), run).await {
                        Ok(Ok(o)) if o.status.success() => {
                            out = Some(o);
                            break;
                        }
                        Ok(Ok(o)) => {
                            // 非零退出:此候选能 spawn(脚本能跑),用其结果不试下一个。
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            let tail: String = stderr
                                .chars()
                                .rev()
                                .take(500)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect();
                            summary.failed += 1;
                            if summary.first_error.is_none() {
                                summary.first_error = Some(format!(
                                    "script {} 非零退出({}):{}",
                                    cfg.script,
                                    cand,
                                    tail.trim_end()
                                ));
                            }
                            break;
                        }
                        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                            // 此候选不在 PATH,试下一个
                            continue;
                        }
                        Ok(Err(e)) => {
                            summary.failed += 1;
                            if summary.first_error.is_none() {
                                summary.first_error = Some(format!(
                                    "script {} spawn 失败({}):{}",
                                    cfg.script, cand, e
                                ));
                            }
                            break;
                        }
                        Err(_) => {
                            summary.failed += 1;
                            if summary.first_error.is_none() {
                                summary.first_error = Some(format!(
                                    "script {} 超时(>300s,可能 codehub-cli 挂起)",
                                    cfg.script
                                ));
                            }
                            break;
                        }
                    }
                }
                let out = match out {
                    Some(o) => o,
                    None => {
                        // 所有候选都 NotFound(无可用解释器)。failed 无条件计
                        // (对齐其余失败分支),first_error 只记首条(守卫内)。
                        summary.failed += 1;
                        if summary.first_error.is_none() {
                            summary.first_error = Some(format!(
                                "script {} 无可用解释器(试过 {})",
                                cfg.script,
                                candidates.join("/")
                            ));
                        }
                        continue;
                    }
                };
                let _ = out; // stdout 不解析,只看 output 文件
                let output_path = Path::new(&proj.workspace_path).join(&cfg.output);
                let raw = match std::fs::read_to_string(&output_path) {
                    Ok(s) => s,
                    Err(e) => {
                        summary.failed += 1;
                        if summary.first_error.is_none() {
                            summary.first_error =
                                Some(format!("script {} 输出读不到:{}", cfg.script, e));
                        }
                        continue;
                    }
                };
                match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(v) => script_outputs.push(v),
                    Err(e) => {
                        summary.failed += 1;
                        if summary.first_error.is_none() {
                            summary.first_error =
                                Some(format!("script {} 输出非 JSON:{}", cfg.script, e));
                        }
                    }
                }
            }
        }

        // Days that already have an observation — fill only gaps (append-only,
        // never rewrite). Shared across the script arm below.
        let mut days_have: std::collections::HashMap<MetricId, std::collections::HashSet<Date>> =
            std::collections::HashMap::new();
        for o in self.store.list_observations(project).await? {
            days_have
                .entry(o.metric_id)
                .or_default()
                .insert(o.ts.date());
        }

        for m in &sigs.metrics {
            // 停用的指标退出自动采集 —— 不拉数、不记点、也不计入本次采集
            // 回执的任何一个计数(它压根没参与,报进去就是虚的)。
            if m.archived {
                continue;
            }
            match m.collect_kind.as_str() {
                // W3-2 第一步:github/codehub/bw/connector legacy kind 一律
                // deferred(不再直采,强走 script connector)。collect_label
                // 已标「legacy·迁script」;第二步(改枚举 + DB 字符串迁移)
                // 放 V1+。北极星(W3-1 后是 script/manual 行)走下面真臂。
                "github" | "codehub" | "bw" | "connector" => summary.deferred += 1,
                "script" => {
                    // plan18-③:从预跑的 script connector 输出 JSON 按
                    // collect_query 字段路径取值。没配 script connector /
                    // 脚本跑失败(已记 failed)→ 这里 deferred,绝不伪造观测。
                    if script_outputs.is_empty() {
                        summary.deferred += 1;
                        continue;
                    }
                    // V2-② Phase B: if any script output carries
                    // `history.<collect_query> = [{ts,v},…]` (Buddy 仓统计),
                    // fill missing calendar days in that series. Same collect
                    // path as「立即采集」/cron — no separate backfill command.
                    let mut filled_from_history = false;
                    for out in &script_outputs {
                        let Some(series) = history_series_for_field(out, &m.collect_query) else {
                            continue;
                        };
                        let have = days_have.entry(m.id).or_default();
                        for (day, value) in series {
                            if have.contains(&day) {
                                if day != today {
                                    continue;
                                }
                                // Today already has a point: only append when
                                // the value moved (same change-guard as the
                                // today-only arm). Historical days stay put.
                                if m.value_raw == value {
                                    summary.unchanged += 1;
                                    continue;
                                }
                            }
                            let ts = if day == today {
                                now()
                            } else {
                                date_at_noon_utc(day)
                            };
                            self.store
                                .append_observation(m.id, SourceKind::Script, &value, ts)
                                .await?;
                            have.insert(day);
                            summary.changed += 1;
                            touched = true;
                        }
                        filled_from_history = true;
                        break;
                    }
                    if filled_from_history {
                        continue;
                    }
                    let mut found: Option<String> = None;
                    for out in &script_outputs {
                        if let Some(v) = json_field_by_path(out, &m.collect_query) {
                            let value = match v {
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            found = Some(value);
                            break;
                        }
                    }
                    match found {
                        Some(value) => {
                            let last_ts = self.store.latest_observation_ts(m.id).await?;
                            let same_window = last_ts.is_some_and(|t| {
                                OffsetDateTime::from_unix_timestamp(t)
                                    .map(|d| d.date() == today)
                                    .unwrap_or(false)
                            });
                            if m.value_raw == value && same_window {
                                summary.unchanged += 1;
                            } else {
                                self.store
                                    .append_observation(m.id, SourceKind::Script, &value, now())
                                    .await?;
                                summary.changed += 1;
                                touched = true;
                            }
                        }
                        None => {
                            summary.deferred += 1;
                            if summary.first_error.is_none() {
                                summary.first_error = Some(format!(
                                    "{}:script 输出无字段 {}",
                                    m.name, m.collect_query
                                ));
                            }
                        }
                    }
                }
                // manual(或空 collect_kind = 界面手建)不归采集器管。
                _ => {}
            }
        }

        if touched {
            self.store.recompute_signals(project, now()).await?;
            self.emit(Event::ProjectUpdated(project));
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod history_series_tests {
    use super::{history_series_for_field, parse_ymd};
    use time::Month;

    #[test]
    fn parses_history_array_for_collect_query_field() {
        let out = serde_json::json!({
            "open_issues": 3,
            "history": {
                "open_issues": [
                    {"ts": "2026-07-14", "v": 0},
                    {"ts": "2026-08-12", "v": 3}
                ]
            }
        });
        let series = history_series_for_field(&out, "open_issues").expect("series");
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].1, "0");
        assert_eq!(series[1].1, "3");
        assert_eq!(series[0].0, parse_ymd("2026-07-14").expect("day"));
    }

    #[test]
    fn missing_history_is_none() {
        let out = serde_json::json!({"open_issues": 3});
        assert!(history_series_for_field(&out, "open_issues").is_none());
    }

    #[test]
    fn parse_ymd_rejects_bad() {
        assert!(parse_ymd("nope").is_none());
        assert_eq!(parse_ymd("2026-08-12").unwrap().month(), Month::August);
    }
}
