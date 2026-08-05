# HANDOFF · V1 Issue 2 · 找指标/绑数据 交互式闭环 — 窗口打结件

> 自足交接件,不读对话也能接手。Issue 2 = 把找指标/绑数据 从"skill 文本打磨"升级成**交互式 claude CLI 闭环 + 绑装置规范**。设计唯一事实源 `docs/v1-prototype/issue2-metrics-interactive-loop.md`(§0 心智模型 / §2.4 orca 模式 / §2.6 收尾决定 #1-#8 / §4 phase 拆分 / §5 文件级 / §6 偏差+遗留)。

## 做了啥(按 phase,全在本 worktree `worktree-v1-issue2-metrics-skill`,未 push)
- **Phase 1**(commit `11a24b9`+`a09e98d`):交互式引擎骨架——`bw-engine/src/interactive_cli.rs`(声明式 CLI 表 CLAUDE支持/cursor占位 + `build_startup_plan` 位置参数 prompt + `build_bridge_system_prompt` 衔接层 system prompt + `InteractiveCliExecutor` 系统终端 spawn + `MockInteractiveExecutor`)+ `bw-app/lib.rs run_issue_interactive` 分流(零扰 one-shot)+ `SettleOutcome::Interactive`。
- **Phase 2a**(commit `b4059ee`):resume(`claude --continue`)+ InReview 检测(轮询 codehub/github open MR,读回为证)+ 状态机(InProgress→InReview[MR查到]→Done[人merge],砍交互式 issue_run_tail 提 MR)+ `issue.interactive_started` 列(schema 双守卫)。
- **Phase 2b**(commit `c2e4099`..`f8fb8b5`):嵌入终端——`portable-pty` PTY + xterm.js widget(`TerminalWidget`,三条 race 解:pre-handler buffer/replayIntoTerminal guard/resize 显式 drain)+ hook listener(`bw-app/src/hook_listener.rs` 127.0.0.1 http + `~/.claude/settings.json` 幂等 hook,抓 SessionStart→`claude_session_id` + Stop→触发 InReview)+ resume 升级 `--resume <session_id>`(F1 用 session_id fallback 修)+ block_on panic 修(`bind()` 同步 + `spawn()` 异步)。review fixup(f8fb8b5):PTY 状态会话后清 None + 移除 `poll_pty_bytes` 双消费 + Windows `curl.exe` + 删死码。
- **Phase 3**(commit `7e8fdc4`+`76c7d0e`):绑装置规范——`.bw/connectors.toml` 解析器(`bw-engine/src/connectors_file.rs`)+ `docs/connectors-toml-format.md` 规范 + `sync_connectors_file_for`(merge 后 buddy 感知 upsert connector 行)+ `connector.project_id`/`config` schema 双守卫补 + collect_kind forward-correct(文档两 kind)。review fixup(76c7d0e):SELECT 加 `kind='script'` 过滤(防同名非脚本串 kind 静默失败)+ 移除「↻同步」手动按钮。
- **Phase 4**(commit `19f2f78`):skill 重写——north-star + metrics-binding SKILL.md 改纯方法论(buddy 契约抽到衔接层 `build_bridge_system_prompt` 唯一持,换业界 skill 当 prefill 产出仍对得上契约)+ metrics-binding script query bug 修(intro/常见坑「query 写脚本路径」→「只写字段路径」,对齐 `docs/metrics-toml-format.md` L88)+ bridge prompt 更新绑装置文件规范。
- **P5 统一 guide**(commit `972588c`):`docs/guide/buddy-guide.html` u3/u4 重写成交互式用户旅程(嵌终端+多轮+agent提MR+buddy检测+merge sync,三段式+系统×CRUD altitude)+ m6 采集章重写(两 kind+.bw/connectors.toml+采集链+sync感知)+ m4/m5 扩到 2b 完整态。

## 锁定的设计决定(设计 md §2.6 #1-#8)
1. buddy 是薄编排器(唤醒会话+灌入阶段 system prompt+skill;skill 驱动交互;衔接层按阶段)。
2. 绑数据通用不为 maas 开后门(skill+system prompt 引导用户在 claude cli 共同开发采集装置)。
3. 维护指南 3 章范围(m4 阶段技能+替换机制/m5 issue调度+claude会话唤醒-resume-多轮/m6 指标采集链+表字段)。
4. 交互式会话=持久+可 resume(点 issue 卡=唤醒会话续聊,F2 补 workflow_run 行作废,改 issue=会话/点卡=resume)。
5. dev 偏差:#1 `--prefill` 解决(位置参数=orca argv 主路径,正确);#2 resume 重设计;#3 预算接受(wall-clock)。
6. m6 待补(已在 P5 补)。
7. 状态机(InReview=PR 关联检测,非跑完;Done 后窗口保持;1 issue=1 session;新 issue 靠读已合入产物文件接上下文)。
8. InReview 检测(读回为证:查 codehub/github open MR,Stop hook 触发,2 GAP 核过)。

## 偏差/未决(设计 md §6)
- F1(2a 已知限制,2b 已修):interactive_started spawn 前置 → 2b 用 session_id fallback 修。
- R1 预算:交互式无 flag cap,wall-clock(用户已接受)。
- R2 kernel 冻死:走 run_issue_backgrounded(已确认)。
- connectors.toml 不删库(同 metrics.toml 语义,留后续 UI 票)。
- CollectKind 枚举代码不收(五值,留采数/总览窗口收两 kind)。
- CDN 依赖(xterm.js 从 jsdelivr,离线不渲染;留 bundle 本地)。
- 真终端渲染 E2E defer 用户(claude+网关 529 抖动)。

## 遗留(本窗口两件,设计 md §6)
- ① 多人协作(多 PC 各装 buddy 纳入同项目):至少别让三件套 issue 重复提;完整协作 V1+。
- ② 制定各规范:buddy 给项目的统一规范(脚本/连接器/skill)扛得住考验;`.bw/scripts/`+`.bw/connectors.toml` 最简规范已定(P3),Hub 几大组件完整规范留遗留单独定。

## 验证状态
- 门禁全过(fmt/clippy/wasm32×2/guard-kernel-ui-free/app-desktop)+ cargo test(33 测:connectors_file 8 + sync_connectors 3 + interactive_cli 19 + core docs 3)。
- code review 全过(Phase 1/2a/2b/3+4 各一轮 SubAgent,F1/Med/Low 均修)。
- **真交互式 E2E defer 用户**:点 ▶跑 → 嵌终端 claude+skill → 多轮 → agent 提 MR → buddy 检测 InReview → merge sync → cron 采数点亮。需真 claude+网关+真项目(用户验)。

## 接手要点
- 守 CLAUDE.md 铁律(UI 无关内核/Done 永不自动/Signal derive-only/settle-once/schema 双守卫/读回为证)。
- 设计决定全在设计 md §2.6;偏差全在 §6。
- 下一步(用户定):真 E2E 验证 → 若 OK,Phase 2b 真终端截图 + 多人协作(遗留①)+ 各规范(遗留②)→ CollectKind 枚举代码收(采数/总览窗口)。
- 或ca 参考:`D:\2026\code\orca-main\orca-main`(声明式 CLI 表/IPtyProvider/PTY ACK/hook→HTTP/session.jsonl 模式,设计 md §2.4 折要)。
