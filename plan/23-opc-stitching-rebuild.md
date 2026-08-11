# 23 · OPC 缝合重建计划:六段总控 + CLI 连接器 + agentcli 层

> **30 秒导读**:本文是 vNext 的**现行执行计划**,给实现者和验收者看,现在作数。它是对 plan/22(codex 原案)的第三轮修订:22 的产品框架(OPC、六段链)被确认采纳,22 的代码诊断核验成立;执行方式按用户 2026-08-10 的三轮拍板收敛为**骨架新建、器官移植**——重头建新工程,但已验证的模块整体搬入、不重写。v1 分支(嵌入终端收口)是最重要的移植来源。
>
> **一句话定位**:BW 是 OPC(one person company,一人公司)时代超级个体的**唯一工作界面**——一个人负责一个完整项目是常态,方向是一人指挥百级到千级 agent、从 chat 走向 loop engineering。BW 把管理体系内置(引领/滞后双指标是中枢)、把干活能力全部缝自业界成熟 CLI,自己只做总控与薄适配。我们不避讳做裁缝。

## 进度实况(持续更新)

| 切片 | 状态 |
|---|---|
| 零 · 登记与横幅 | ✅ 830b5bc |
| 一 · 骨架与器官移植 | ✅ A/B/C 五 commit(0f032b9…d9504e3),bw-core+四器官零改写移植,读回指挥器 PORT_READBACK_OK |
| 二 · 连接器地基 | ✅ A/B/C 五 commit(2b9e16a…f799e17),契约+gh/codehub/脚本三家+probe_all 真探活+CI next-gates |
| 三 · agentcli 层 | 🔄 三-1 完成(契约三档/PROTOCOL=2/PTY 双平台后端+进程组真杀,0eb0c2b…a1d9499);三-2 完成(注册表/claude 连接器/指挥器,0099fc0/a8a0641/0075187)+ 三-2 修复轮完成(会话号真接线/铁律正文进系统提示词/断言真空补齐/取消真杀进常绿,§10 第 5 条)+ 缺口清偿轮(env-strip 真生效 + Unix 自动提交,97a66a6,§10 第 3/4 条已消灭);`--real` 端到端仍未跑通——验证轮新发现一道独立阻塞(全新工作区首次交互式启动的双重确认对话框,§10 第 6 条,待设计取舍);切片三主体收官,四待发 |
| 四 · 并行运行 | ✅ 四-1(216a424/d38462c+修 f8d3bef)三表+部分唯一索引+比较并置守卫,全进突变自证断言面,分层守卫改查真实依赖图;四-2(dfd71d3/ad252fc/062e75a/774c3cb)运行管理器单口+十件并行五竞态;修复两轮(f274bac/647bba6/6deab6c)根治验收壳假绿(失败必红+条数等式守卫)、降级与晚到消息真覆盖、管理器诚实收尾——opus 评审曾判不通过,七组突变检验后闭合,mock 档 196-256 条断言常绿;四E 真实并行三件如实撞 §10 第 6 条(双重确认对话框)阻塞,证据留本机不入仓 |
| 五 · 六段总控 + 待人处理 | ✅ 设计定案(design-s5-hexpanel.md,主控裁决 12 条:两屏制/北极星进指标表 v1 f0a187a 修法/信号不落库/待人处理五项各有消失条件/嵌入终端推后钉为切片 5.5);五-1(bw-workspace crate+存储增量,dcf9b18/b308407+修 17d8eb0)完成;五-2/五C/五D(命令/事件总线按聚合拆+六段推导 c6fe4cd、待人处理五项投影+hex_readback 指挥器 4d82942+修复轮 682e625)完成,§10 第 9 条清偿;五-3/五E(桌面壳立起,两屏+深链+壳不持有时钟+三条新守卫,9c2dfcc+修 14620ba:时钟守卫拦分组导入/指挥器走生产写入路径/逃生舱命令加引号)完成,§10 第 16 条清偿(`Command::SetIssueStage` 生产写入方);每步均经 opus/sonnet 评审与突变检验闭环——切片五收官;五份切片设计稿(含主控裁决)已归档 `docs/design-next/` |
| 六 · multica cli 穿刺与缝入 | ⬜(等用户提供仓库指针) |
| 七 · 真实项目切换 + 一次性导入 | ⬜ |

## 0. 三轮拍板(2026-08-10,用户定;本文全部落实)

**第一轮**:① 单人优先,多人走代码分支协作,不建成员/权限/审批,通知走连接器接现成通信软件;② 单元测试依旧不写,验证纪律=E2E(深链+读回+computer-use)+ /code-review;③ 兼容已合入的嵌入终端方案(orca 模式)。

**第二轮**(GPT 5.6 审阅五条,核实全部成立):运行竞态要由独立的运行管理器统一收口;本地工作区读取不是连接器;「待人处理」推导优先、不建告警表;连接器要有最小机器契约;并行运行要验五个竞态场景。本版全部保留。

**第三轮**(本版主因):① **OPC 概念正名**——one person company,超级个体,一人负责一个完整项目是常态,这是产品瞄准的对象(词表已登记);② 六段链回到 22 原链:**项目目标 → 五角色责任 → 引领指标 → 当前 Loop → 风险与决策 → 交付证据**;③ **确认重头构建**——旧工程冗余多,且能力已外移到 CLI,新工程只剩总控+连接器,体量足够小,重建可行;④ **所有 CLI 都是连接器的一种**;⑤ 立 **agentcli 层**:agent 类 CLI(claude、cursor……)单独一层统一驱动;⑥ 能力底座=GitHub 上推荐的成熟实现,选型优先于自建,**举证责任在自建一方**。

**第四轮**(理念定调,本计划的「为什么」):① **做减法是纲**——旧玩法太膨胀,无序扩张导致沉淀不下来;每个切片都要回答「这让工作界面更少了还是更多了」。② **workbench 是 OPC 的唯一工作界面**:目标是降低超级个体的工作界面数量,能力集成进来,而不是人跳出去各处开工具。③ **引领/滞后双指标是项目管理最重要的技巧**,也是六段链的中枢:引领指标驱动本周行动,滞后指标验证方向,北极星统摄两者。④ **面向 agent 编程**:所有知识浓缩在一个 codebase——项目仓是唯一事实源,BW 的蒸馏、注入、标准文件、指标正本全部围着项目仓转,不建第二个知识库。⑤ **native 团队愿景**(Boris 对 agent 原生团队的刻画):一人管理 100~1000+ agent,从 chat 走向 loop engineering——BW 内置「把 workflow loop 建立起来并持续优化」的能力,这是「当前 Loop」一段的终局形态。⑥ v1 成功集成 orca 已证明缝合路线可行;**其余能力的集成验证由 Claude 执行**(探索 → 穿刺留档 → 定档 → 缝入 → 读回验证),入选前提是候选具备 CLI 执行能力。

**第五轮**(执行期定位补充,2026-08-10 晚):**tree/v1 = 多人协同、其他人真实使用场景的适配**——v1 不是待淘汰的参考料,是伙伴维护的活分支,服务多人场景的真实用户;next/ 主线只服务单人 OPC。两线互不吞并(成员/多人语义留在 v1,不进 next/),各自演进、通过代码分支协作——这正是「多人协作走代码分支」原则的自身实践。由此三条推论:① **v1 分支永不归档**,切片七归档的只是 main 上的旧应用;② 器官移植只取件,不接管 v1 的多人特性;③ 移植件里发现的缺陷(见 §10 台账)附证据登记,可回报伙伴修 v1,我们不擅改 v1 分支。

## 1. 为什么这次重建可行(与 22 原案的重建有何不同)

22 的重建=一切重写,连已验证的领域内核都不复制——那太贵,也没必要。本计划是**骨架新建、器官移植**:

- **新建的是工程结构**:小命令分发器(按聚合拆,不再是近万行单 match)、按聚合的存储接口(不再近百方法一个 trait)、薄桌面壳(UI 不持有调度时钟、不参与运行调度——22 诊断的三处结构病,新骨架从第一天就不犯)。
- **移植的是已验证器官**(搬文件+适配接口,不重写逻辑,清单见 §5.3):五阶段元数据与状态机、密封信号派生、v1 终端会话栈、采证器、指标正本管道、gh/codehub 适配。
- **旧工程为什么不续**:除结构病外,更关键的是旧工程一半以上的功能面(工作流引擎、三类 Hub 屏、看板、创建向导、Mock 运行路径)不在新范围内(§6 退役清单)——续着改等于背着不要的东西前进。
- **护栏**(防重建烂尾的老病):旧应用保持可跑,作为行为参照与回退(22 的策略,保留);第一条真实纵向链路跑通前不追求功能对等;新工程放仓内 `next/` 独立 workspace(22 的策略,保留),与旧工程并存;砍掉的功能如实列明,不假装「以后都会有」。

## 2. 产品中心:OPC 六段总控

总控界面固定按六段组织(22 原链):

```text
项目目标(北极星一句话与项目要务)
→ 五角色责任(原型师/构建师/优化师/运营推广师/运维师:该问什么、做到什么算完)
→ 引领指标(北极星 → 滞后 → 引领,引领卡带本周目标)
→ 当前 Loop(围绕目标长期在跑的机制与它们的运行,不是对话墙)
→ 风险与决策(记录在案的风险、拍板与带险交棒的欠账)
→ 交付证据(MR / commit / 产物 / 观测,每个数字能点开原始出处)
```

- **五角色责任在 OPC 语境下的含义**:常态是一人分饰五角——界面如实展示五个责任视角各自的核心问题、方法环节与完成清单(全部来自五阶段元数据,移植不重写),**不建成员表**;偶尔多人协作走代码分支,作者身份从 git/MR 证据如实显示。
- **双指标是中枢**:引领指标驱动本周行动(引领卡带本周目标),滞后指标验证方向,北极星统摄两者;指标正本住在项目仓(`.bw/metrics.toml`),与「知识浓缩在一个 codebase」同一原则。
- **当前 Loop**:采纳 22 的定义——Loop 是围绕项目目标长期运行的机制,Run 是 Loop 的一次执行;agent 干活的过程细节留在上游 CLI 里,总控只收规范化状态、评审门与证据。词表处置见 §7 登记项(旧义「工作流内部重试」随旧工程退役,不混用)。**规模方向是百级并行**(native 团队愿景:一人管 100~1000+ agent):首版只验收十件并行(切片四),但所有架构决策——按运行编号管理、待人处理推导投影、评审门批量处理、界面聚合展示——不得内含「同时只有几个 run」的隐藏假设;一人管百级 agent 时,界面呈现的是聚合状态与例外,永远不是一百面终端墙。
- **风险与决策**:进域模型,只追加、事后抹不掉(与交棒记录同族);「待人处理」清单仍是**推导投影**(第二轮结论):风险/决策/欠账是数据,清单是从数据实时推导的视图,状态恢复行自然消失,绝不做越积越多的告警表。
- 绿色保持安静,红/黄/未知/待人的才出声;六段每个数字可 `sqlite3` 读回。
- v1 总览重构(W3)的版面成果(北极星常驻顶栏、指标三层卡、诚实灰)作为六段 UI 的设计参照移植。

## 3. 连接器:一切对外能力(所有 CLI 都是连接器的一种)

**分类**(按能力家族,不按厂商):**执行连接器**(agent 类 CLI,经 agentcli 层驱动,见 §4)· **仓连接器**(gh、codehub-cli)· **协作连接器**(multica cli)· **采集连接器**(业务脚本→观测,沿用 `.bw/connectors.toml` 正本)· **通信连接器**(候选,多人协作的通知出口)。

**结构**(第二轮结论,保留):统一的登记与分发入口 + 按能力拆的小接口(探活 / 执行 / 采集 / Issue·MR 操作),连接器各自声明支持哪些,不支持的如实报。**本地工作区读取不是连接器**(词表 V1 定义):采证是内建函数,不进连接器接口。

**「有些 CLI 不太好直接用」的对策——三档接法**,选型时逐个定档:

1. **结构化直包**:CLI 有机器可读输出(`gh --json`),薄适配归一化即可;
2. **agentcli 层驱动**:交互式 TUI 类(claude、cursor),走 PTY 会话 + prompt 注入 + hook 完成信号(§4);
3. **外开 + 事实回收**:两者都不行的,深链/外部打开,状态靠 git、MR、hook、产物文件回收——绝不抓屏幕文本,绝不冒充已同步。

**最小机器契约**(钉在 Rust 接口层;外部工具原生输出管不了,契约约束适配器归一化后交回内核的东西):协议版本与能力名;请求编号,可重试写操作带防重编号(同一请求重发不做两遍);结构化成功/失败响应,失败带错误分类;超时与取消行为明确;项目绑定身份。只说「stdout 输出 JSON」不足以防各连接器重新长出字符串分支——契约是防复发的闸。

**纪律**:stdout 只说机器话,stderr 才是诊断;凭证只放系统钥匙串或本机配置,不进项目仓不进日志;连接器坏了只进「待人处理」,绝不写假数据、假零、假绿;「装了」≠「连上了」,探活通过才叫连接。

**选型清单**(能力底座登记表;新能力需求先查这里,清单没有先找业界实现穿刺留档,确实没有才准自建且写明理由):

| 上游实现 | 接法档位 | 状态 |
|---|---|---|
| `gh`(GitHub 官方 CLI) | ① 结构化直包 | ✅ 在用,切片二收编 |
| `codehub-cli` | ① 结构化直包 | ✅ 在用,切片二收编 |
| `claude` CLI | ② agentcli 层 | ✅ 在用(v1 终端栈),切片三移植接通 |
| cursor CLI | ② agentcli 层 | 🔜 切片三第二家(v1 注册表已留位) |
| orca-main | 模式移植 | ✅ 四模式已借入 v1(注册表/hook 信号/jsonl 采证/prefill 注入);本体不接 |
| multica cli | 待穿刺定档 | 🔜 切片六:穿刺留档后缝入 |
| Open Design | 待穿刺(可能要 MCP) | ⬜ 候选 |
| Kandev | 待穿刺(AGPL 未评估) | ⬜ 候选 |
| OpenWork / OpenWorker / WorkBuddy | 待穿刺 | ⬜ 候选 |
| 通信软件 | 待选型 | ⬜ 候选 |

清单只会越选越多;用户新推荐的实现随时登记,登记先于使用;穿刺一律读真实实现,不基于 README 猜。**集成验证由 Claude 执行**,流程固定:探索(定位仓库 + 读真实实现)→ 穿刺档案落 `docs/` → 按三档定接法 → 缝入 → 指挥器读回验证。候选仓库地址以用户提供为先(multica 等推荐实现本机均无 clone);没有指针的由 Claude 检索定位、先向用户确认是同一个项目,再穿刺。

## 4. agentcli 层:agent 类 CLI 的统一驱动(器官移植自 v1)

**是什么**:一张声明式注册表驱动多家 agent CLI——每家一行配置(启动方式、prompt 注入模式、resume 方式),加新 CLI = 加一行配置,不是写一个新实现。这是 orca 的注册表模式,v1 已在 Rust 落地(启动计划构造器,起步挂 claude + cursor 两条)。

**组成**(全部有 v1/orca 验证过的对应物,移植为主):

- **注册表与启动计划**:每家 CLI 的参数拼装、prompt 注入模式(argv / flag / 追加系统提示词)——移植 v1 `interactive_cli.rs`;
- **会话管理**:每会话独立 PTY、字节按会话路由、有界输出环、尺寸同步——移植 v1 `terminal_manager.rs`;
- **会话身份持久化**:会话表(项目 / 活 / 上游会话号 / 工作区路径 / 分支)——移植 v1 `claude_conversation` 语义;含硬事实:claude 按工作目录编码存会话,resume 必须同路径重建 worktree;
- **完成信号**:hook → 本地 HTTP → 事件(orca 模式,比等进程退出可靠);
- **会话证据**:session.jsonl 采集器(首句 prompt、token 用量、子任务状态),不自己重算,信上游数据;
- **复利链接线**:蒸馏块 / 技能正文 / 目录块经系统提示词注入,用量记账结算一次——任何 agent CLI 跑活,这条链不许退化。

嵌入终端界面(每卡独立 xterm、焦点同步、恢复中提示)随 agentcli 层进入新桌面壳——「兼容已合入方案」在重建语境下的含义就是**整体移植**:模式、模块、语义一起搬,伙伴的成果一行不废。

## 5. 工程骨架(新建部分)

### 5.1 结构

仓内 `next/` 独立 Cargo workspace,与旧工程并存。crate 划分沿用现有五层心智(内核 / 引擎 / 存储 / 编排 / 壳),但吸取 22 诊断:命令分发按聚合拆模块;存储接口按聚合拆 trait;信号重算独立于存储实现;桌面壳只发命令收事件,不持有调度时钟。内核保持零 IO、wasm 可编译,门禁照搬(fmt / clippy / wasm check / 内核无 UI 守卫)。

### 5.2 领域与运行

- **新库**:范围缩小后新开 schema(观测 / 交棒 / 风险 / 决策等 append-only 表 + 双守卫迁移那套老规矩);旧库不自动升级,一次性导入器后置到切片七,只导 Project / 目标 / 指标 / 观测 / Issue / 产物证据 / 交棒,逐实体计数 + 人工确认。
- **运行管理器**(第二轮结论,新骨架第一天就内建):启动、取消、结束回写、重启后遗留清理走同一个口;按运行编号登记;取消与完成撞车、结果晚到由它串行化;「同一件活一次只有一个交付运行」,同项目并行放开;「降级为咨询」语义(v1)保留。
- **待人处理**:推导投影(通知页机制移植:无自有表,行随底层状态消失)。

### 5.3 器官移植清单(搬,不重写;移植后行为用指挥器读回验证)

| 器官 | 来源 | 去处 |
|---|---|---|
| 五阶段元数据 + Issue 状态机 + 密封信号 | `crates/bw-core`(`model.rs` / `derive/`) | next 内核,近乎原样 |
| 终端会话栈 | v1 `terminal_manager.rs` / `interactive_cli.rs` / `claude_conversation` | agentcli 层(§4) |
| 采证器 | `bw-engine/src/evidence.rs` + v1 session.jsonl 方案 | 采集连接器与会话证据 |
| 指标正本管道 | `bw-engine/src/metrics_file.rs` + 停用不物删语义 | 指标域(`.bw/metrics.toml` 仍是正本) |
| gh / codehub 适配 | `bw-engine/src/github.rs` / `codehub.rs` | 仓连接器(包进小接口) |
| 迁移双守卫模式 | `sqlite.rs` `add_column_if_missing` | 新存储层 |
| 结算一次守卫 | `settled_at` COALESCE 模式 | 运行管理器 + 存储层 |

## 6. 退役清单与明确不做(如实列,不假装以后都会有)

**随旧工程退役、不进 next**:工作流引擎与 WorkflowSpec、三类资产 Hub 屏(浏览面并入按需选择器)、看板与创建向导、Mock 运行路径(演示只在指挥器里,自我标注)、旧七面板布局。

**明确不做**:不建成员/群聊/消息中心;不自研看板拖拽、甘特、画布、代码编辑器、Diff;不建订阅/游标/事件重放(轮询够用);不做 SaaS 市场/OAuth;不写单元测试;不引入没有穿刺档案的外部组件;编排层不准出现连接器字符串分支或直接进程调用(对外能力只准走连接器接口);**不把已包装的成熟 CLI 改写成原生实现**——接口后面永远是上游工具本尊,不是自研替身。

## 7. 执行切片(每片一到数个可读 commit;不新开字母代号)

- **切片零 · 登记与横幅**:本文、plan/README、plan/22 横幅、词表(OPC 已登记;「待人处理」已登记;Loop 新义的 ADR 台账登记随切片一落地时办,旧义在旧工程退役前不删)。
- **切片一 · 骨架与器官移植**:`next/` workspace 立起,内核与引擎器官(§5.3 前四行)移植编译过,门禁接上。验收:门禁全过;状态机 / 信号派生 / 指标管道用 headless 指挥器读回,与旧工程行为一致。
- **切片二 · 连接器地基**:登记分发 + 小接口 + 最小契约;gh / codehub / 脚本收编。验收:指挥器实跑探活 + 读回;删除任何一种连接器,内核编译与其余连接器不受影响。
- **切片三 · agentcli 层**:终端栈移植,claude 首家接通:真实活卡一键跑 → 提 MR → 评审中,复利链注入与记账读回;cursor 第二家只加一行配置,证明注册表成立。
- **切片四 · 并行运行**:运行管理器。验收(确定性指挥器,MockExecutor 可配延迟造竞态):十件活并行互不覆盖、各自只结算一次;同一件活开不出第二个交付运行;取消与完成同至只结算一次;单条失败不牵连;重启后遗留运行如实标注不假活;晚到消息不错账;真实并行三件;「完成永远人点」不变。
- **切片五 · 六段总控 + 待人处理**:§2 的 UI 与投影。验收:深链启动 + stderr 渲染证明 + 截图;六段每个数字读回;无数据如实显灰。
- **切片六 · multica cli 穿刺与缝入**:先档案后代码(穿刺可提前与任何切片并行);缝或不缝按档案结论,不缝则候选池除名并写明原因。
- **切片七 · 真实项目切换 + 一次性导入**:选一个真实项目在 next 里跑完整六段链后,才允许归档旧应用(22 的完成标准,保留)。归档对象只是 main 上的旧应用;**v1 分支不在归档范围**(它是多人协同场景的活分支,见开头「第五轮」定位)。

顺序:一 → 二 → (三、四) → 五 → 七;六的穿刺随时可做。**v1 合入主线建议先办**(移植来源有主线身份、伙伴成果留档清晰),但不阻塞切片一。

## 8. 验证纪律(不变)

E2E(深链 + sqlite 读回 + computer-use)+ /code-review,不写单元测试;连接器与移植行为的验证 = headless 指挥器实跑 + 读回;真实外部调用(网关、multica 服务)不进常绿门禁,只在指挥器/监理脚本里跑,可安全重试;mock 一律自我标注。

## 9. 工程对照表(计划用词 → 代码锚点;实现机制词只住在这里)

| 计划用词 | 代码锚点 |
|---|---|
| 旧工程结构病(重建理由) | `bw-app/src/lib.rs`(9,872 行单分发器)、`bw-store/src/lib.rs`(98 方法 Store)、`app-desktop/src/kernel.rs`(UI 层调度时钟) |
| 串行锁(新骨架废除) | `bw-app/src/lib.rs` `run_issue_now` 守卫 + `AppState.active_run` |
| agentcli 层移植源 | v1 `bw-engine/src/terminal_manager.rs`、`interactive_cli.rs`、`claude_conversation` 表 |
| 待人处理的推导先例 | `ui/src/vm.rs` `notify_feed` + `attention_from_rows`;`app-desktop/screens/notify_hub.rs`(无自有表) |
| 复利链 | `DistillSkillFromIssue` → 注入(`standard_refs`/`distilled_refs`)→ `record_skill_use` |
| 信号只能推导 | `bw-core/src/derive/sealed.rs`(密封 `Derived<Signal>`) |
| 完成永远人点 | `bw-core/src/model.rs` `can_transition_to` |
| 迁移双守卫 / 结算一次 | `sqlite.rs` `add_column_if_missing` / `settled_at` COALESCE |
| 指标与采集正本 | `.bw/metrics.toml`、`.bw/connectors.toml`(格式文档见 docs/) |
| 六段 UI 设计参照 | v1 `docs/v1-prototype/issue3-overview-refactor.md` + 高保真原型 |

## 10. 已知缺口(执行中登记,只追加)

> 执行中途发现、暂不消灭的缺口登记在这里,只追加不修改;缺口一旦消灭,在对应行末尾补一句「已消灭于 commit XXX」,不删除原记录(如实留痕,与交棒记录同族——事后不抹)。

1. **Windows PTY 收尾正常路径必 panic**(登记日 2026-08-10)
   - **现象**:`next/crates/bw-engine/src/pty_backend.rs` `windows::WindowsPtyBackend::run` 的收尾代码——`tokio::select!` 循环里 `_ = &mut read_handle => break`,循环外再无条件 `let _ = timeout(..., read_handle).await`——一旦读循环那支先完成(子进程正常退出、读到 EOF 是最常见的收尾路径),`read_handle`(`tokio::task::JoinHandle`)已经在 `select!` 里被轮询到 `Ready` 过一次;循环外再 `.await` 同一个 `read_handle` 会 panic「JoinHandle polled after completion」。正常收尾路径必触发,不是边缘情况。
   - **来源**:v1 `interactive_cli.rs` `run_skill_pty`(`#[cfg(windows)]` override)整段搬运件,零改写移植(next 切片三B)。这个问题在移植前就存在于 v1 源码里,不是移植过程引入的新 bug;`unix::UnixPtyBackend`(本片新写,不受零改写约束)已经用 `read_finished` 标志位避开了同一个坑。
   - **待办**:需要一台 Windows 机器验证真机行为,再按 Unix 侧已经用过的思路(拆分「哪支 break 出循环」,只在读循环没被 `select!` 消费过时才 `.await`)修。
   - **登记日**:2026-08-10。

2. **`ExecSpec` 没有独立的「这件活要干什么」字段**(登记日 2026-08-10)
   - **现象**:`bw-connector` 契约的 `ExecSpec`(切片二冻结)只有 `workspace`/`branch`/`inject`/`budget_usd` 四个字段;`inject: Vec<InjectBlock>` 按设计口径整体进系统提示词(`--append-system-prompt`),没有一个字段对应 v1 `interactive_cli.rs` `build_startup_plan` 的 `position_prompt`(issue 标题+描述,首启时作为位置 prompt 自动提交、真正触发 agent 开始动手的那句话)。
   - **来源**:切片二骨架阶段定型 `ExecSpec` 时,agentcli 层(切片三)尚未接线,没有把「任务正文放哪」这个问题摆到台面上。`bw-engine/src/agentcli/connector.rs` `AgentCliConnector::start` 首启时因此用一条固定的通用开局句(`GENERIC_KICKOFF_PROMPT` = "请阅读上面的系统提示词并开始执行。")当位置 prompt——不编造任务内容,但也不携带真实任务正文,首启的第一条用户消息只会指向系统提示词里的 `inject` 内容。
   - **待办**:切片四编排层(运行管理器)真正把 Issue 交给 `Execute::start` 时会正面撞上这个缺口——要么在 `ExecSpec` 加一个任务正文字段(撞协议号,按契约冻结规矩来),要么定一条「`inject` 的某个特定标签就是任务正文」的调用约定。哪种取舍是产品/协议层面的决定,不该由 agentcli 层单方面通过改契约形状解决,留给切片四接线时定案。
   - **登记日**:2026-08-10。

3. **`pty_backend.rs` 的 env-strip 对真实子进程不生效**(登记日 2026-08-10,切片三E 实测发现)
   - **现象**:`build_startup_plan`/`build_resume_plan`(`interactive_cli.rs`)把嵌套会话相关的几个环境变量(`CLAUDE_CODE_CHILD_SESSION` 等)从 `LaunchPlan.env` 这个 `HashMap` 里删掉,文档注释与既有任务报告都把这当作「已经生效的防护」。**实测(临时脚本,验证后已删,不入本 commit)证明它对 `pty_backend::unix::UnixPtyBackend::run` 的真实子进程无效**:该函数用 `portable_pty::CommandBuilder::new(binary)` 起步,而 `CommandBuilder::new` 在构造时就用 `get_base_env()` 把**当前进程的完整环境**(`std::env::vars_os()`)整个复制进它自己内部的 env 表(portable-pty 0.9.0 `src/cmdbuilder.rs`);随后 `pty_backend.rs` 只对 `plan.env` 里**剩下的**键调用 `cmd.env(k, v)`(逐条覆盖/新增),从未调用 `cmd.env_remove(...)` 或 `cmd.env_clear()`——`plan.env` 里被删掉的键因此从未真正从 `CommandBuilder` 自己那份「已经复制了全量环境」的内部表里移除,子进程照样原样继承。用一个哨兵环境变量 + 起一个真跑 `env`(打印环境）的子进程直接验证:该变量与 `CLAUDE_CODE_CHILD_SESSION` 均**原样出现在子进程输出里**,证明「删了」没有真的删掉。
   - **来源**:`pty_backend::unix::UnixPtyBackend::run`(切片三B 新写,不是 v1 零改写移植件),以及同样模式的 `interactive_cli.rs` `InteractiveCliExecutor::run_skill`(`tokio::process::Command` 路径,v1 零改写移植——`tokio::process::Command` 默认同样整段继承父进程环境,`.env(k,v)` 只覆盖/新增,不清空)。**Windows 侧(`pty_backend::windows::WindowsPtyBackend::run`,conpty-oxide)未实测,但同一模式(整段搬运、未见 `env_clear`)大概率同样中招**,如实标注未验证,不假设它没事。
   - **为什么切片三-1/三A 的 `--session-id` 实测没有先发现这个坑**:上一任务(切片三-1)验证 `--session-id` 用的是一段独立的 Python 探针(`pty.openpty()` + `subprocess.Popen` 直接起 `claude`),完全绕开了 `bw-engine` 自己的 `pty_backend.rs`/`interactive_cli.rs` 代码路径;该探针靠在**外层 shell** 用 `env -u ...` 剥离变量后再起 Python 进程才成功,这个外层剥离掩盖了「BW 自己的 Rust 代码其实没有正确剥离」这个事实——本片(切片三E)是第一次真的跑通 `pty_backend.rs` 的真实 spawn 路径去验证这件事,才第一次发现。
   - **本片(切片三E)如何绕开**:`agent_session --real` 档验证时,在**外层进程调用**上用 `env -u ...` 剥离(与三-1 报告同一份变量清单),不依赖 `plan.env` 这条(已知失效的)内层剥离——绕开不是修复,如实标注。
   - **待办**:`pty_backend.rs`(unix/windows 两份)的 spawn 都应该改成先 `cmd.env_clear()`(或 `CommandBuilder` 对应的清空 API)再整个用 `plan.env` 重建环境,让「删了」真的生效;`interactive_cli.rs` `run_skill` 的 `tokio::process::Command` 路径是 v1 零改写移植件,同样的问题按「移植件不擅改」的既有规矩,留给读到这条登记的下一个会话按当时的裁决(修 vs 继续零改写)处理,不由本片单方面决定。这条缺口影响面比字面看起来大:任何经 `pty_backend` 真正 spawn 出去的 claude 会话,只要 BW 自己运行在另一个 Claude Code 会话内部,子会话都会继承宿主的鉴权/网关/会话号,可能导致 401 或 transcript 被关闭(与三-1 报告记录的首次失败现象一致)。
   - **登记日**:2026-08-10。**已消灭于 commit `97a66a6`**(「next 切片三-2 修三 · env-strip 真生效(unix/windows)+ Unix 自动提交进常绿」):`pty_backend.rs` 的 `unix::UnixPtyBackend::run`/`windows::WindowsPtyBackend::run` 起步都加了 `cmd.env_clear()`,`plan.env` 真正成为子进程环境的唯一来源;`pty_smoke` 指挥器新增确定性 env-strip 断言节(不依赖网关,已做突变自证)。范围如实标注:只覆盖 `pty_backend.rs` 两份 PTY 后端;`interactive_cli.rs::InteractiveCliExecutor::run_skill` 的 `tokio::process::Command`(非 PTY)路径仍是同一模式的隐患,按待办里定的规矩不在这次动。Windows 侧只用 `cargo check --target x86_64-pc-windows-gnu` 交叉编译核对过类型,未在真机验证。

4. **Unix 后端不自动提交位置 prompt,真实会话验证时首次实测确认会卡住**(登记日 2026-08-10;此前是切片三-1 报告的一条 concern/开放猜测,本条是它的实测坐实)
   - **现象**:`agent_session --real` 跑了一次真实首启(`AgentCliConnector::start` 经真实 `InteractiveCliExecutor`/`pty_backend::unix` 真 spawn 了 `claude`),`start` 本身成功、`upstream_session` 拿到了自己指派的 uuid,但轮询 90 秒始终停在 `Running`,最终按超时兜底取消;事后读回 `~/.claude/projects/<encoded>/<uuid>.jsonl`——**文件从未被创建**(不是空文件,是压根不存在),说明 claude 连第一轮交互都没发生。
   - **来源**:`pty_backend::unix::UnixPtyBackend` 是切片三B 明确写清楚的「最小集」——**刻意不含** Windows 实现里那段「TUI 加载完等 2 秒自动发 `\r` 提交位置 prompt」的补丁(模块文档原话:「真要在 macOS 上验证是否也需要这个补丁,得先有一次真实交互式 claude 会话观察,留给切片三 C/D 接线后」)。本条就是那次观察:位置 prompt 确实以 argv 形式传给了 `claude`,但没有一次真实按键把它从输入框送出去,会话因此一直停在「TUI 起来了、在等用户按 Enter」这一步,不会往前走。
   - **待办**:`pty_backend::unix::UnixPtyBackend::run` 需要照 Windows 那段逻辑的思路(TUI 起来后等一小段时间、`plan.submit_prompt` 为真时发一次 `\r`)补上对应的 Unix 版本,补完后需要再跑一次 `agent_session --real` 验证 jsonl 真的落地且非空、且里面第一条记录是真实的位置 prompt 正文。
   - **补充(2026-08-10,切片三-2 修复轮)**:本条(自动回车)只是 `--real` 从没跑通的**其中一个**独立阻塞——见下方第 5 条,当时还有另一个独立阻塞(会话号从未真交给 claude),已在本轮修复。两个阻塞都不解除,`--real` 才会真的跑通一整轮。
   - **登记日**:2026-08-10。**已消灭于 commit `97a66a6`**(「next 切片三-2 修三 · env-strip 真生效(unix/windows)+ Unix 自动提交进常绿」):`unix::UnixPtyBackend::run` 补上了 `submit_delay`/`submitted` 结构,语义对齐 Windows。`pty_smoke` 指挥器新增确定性断言节证明后端确实会自动发 `\r`(不依赖网关)。**但补完后按待办要求重跑 `agent_session --real` 发现:本条缺口的修复是必要但不充分的**——jsonl 依然没有生成,根因不是本条描述的「没人按 Enter」,而是另一个更深的、新发现的阻塞(全新工作区首次交互式启动会先弹两个交互确认对话框),已追加登记为下方第 6 条,不算在本条清偿范围内。

5. **`build_startup_plan` 从没读过 `session_id_flag`,`--session-id` 从未真正交给 claude**(登记日 2026-08-10,发现于切片三-2 修复轮;已消灭于本轮同一次修复——`build_startup_plan`/`AgentCliConnector::start` 均已改,见下方待办)
   - **现象**:切片三C/D/E 三个 commit 把 `session_id_flag: Option<&'static str>` 字段加进了 `TuiAgentConfig`(注册表行,`interactive_cli.rs`),`CLAUDE` 行也填了 `Some("--session-id")` 并在文档里写「切片三-1 已用真实交互式会话验证过接受这个旗标」——但 `build_startup_plan` 的函数体当时**从没读过这个字段**,`agentcli::connector::AgentCliConnector::start` 首启时在内存里编了一个 uuid 塞进 `SessionRow.upstream_session`/回传的 `ExecTicket`,却从没把这个 uuid 放进 `claude` 的 argv 里。结果是:claude 会用它自己生成的会话号落 jsonl,BW 票据上记的那个「upstream_session」是 BW 自己编的、claude 根本不认的一个号——任何拿这个号去 `~/.claude/projects/<encoded>/<uuid>.jsonl` 读回的验证必空,不是因为文件慢生成,是因为这个文件从一开始就不会用这个文件名生成。
   - **为什么当时没被三-1 的实测发现**:三-1 验证 `--session-id` 旗标本身「交互模式下真被接受」用的是独立 Python 探针(`pty.openpty()` + `subprocess.Popen` 直接起 `claude --session-id <uuid> ...`),这条探针自己拼好了完整 argv(含 `--session-id`),证明了「claude 这个旗标真能用」,但没有经过 `bw-engine` 自己的 `build_startup_plan`/`AgentCliConnector::start` 代码路径——三C/D/E 把这个字段加进类型、写好了文档,却漏了把它接进函数体这一步,直到三-2 修复轮实测 `agent_session --real`(见上方第 4 条)才第一次真的跑通这条代码路径,同时发现这个漏接线的问题。
   - **来源**:`next/crates/bw-engine/src/interactive_cli.rs` `build_startup_plan`;`next/crates/bw-engine/src/agentcli/connector.rs` `AgentCliConnector::start` 第③步(编会话号那一段)。
   - **待办(已在本轮完成)**:`build_startup_plan` 加 `session_id: Option<&str>` 形参,`agent.session_id_flag` 与调用方传入的 `session_id` 都非空才推 `--session-id <id>` 进 argv;`AgentCliConnector::start` 只有 `row.session_id_flag.is_some()`(这家真支持指派)才编 uuid、否则 `upstream_session` 如实留空(`String::new()`,`ExecTicket.upstream_session` 回 `None`),不再无条件瞎编一个号。指挥器(`agent_session.rs` 第 1 节)补了「给了 session_id」与「session_id 传空串不推旗标」两条逐字节断言。
   - **登记日**:2026-08-10。已消灭于本次「next 切片三-2 修 · 会话号真接线 + 铁律正文进系统提示词 + 断言真空补齐 + 取消真杀进常绿」commit。

6. **全新工作区首次交互式启动,claude 在正文之前会先弹两道交互确认对话框,阻塞 `--real` 端到端跑通**(登记日 2026-08-10,发现于第 3/4 条清偿轮验证)
   - **现象**:第 3 条(env-strip)与第 4 条(自动提交)按各自待办修完、指挥器断言全绿之后,`agent_session --real` 重跑仍然 90 秒轮询不到 `Finished`,上游 jsonl 依旧没有生成——说明这两条缺口的修复是必要但不充分的,还有第三个独立阻塞没被发现过。用一个不入 commit 的临时探针(直接调 `pty_backend::active()` + `build_startup_plan`,实时打印 PTY 字节到屏幕)复现:全新 git 工作区首次以 `--dangerously-skip-permissions --session-id <uuid>` 交互式启动 claude 时,在进入正常聊天界面之前,依次弹出两道交互确认——① 工作区信任确认(「是否信任这个文件夹」,默认选项是「是,信任」)、② bypass-permissions 模式警告(说明该模式下不再逐次询问危险操作的确认,**默认选项是「否,退出」,不是「是,接受」**)。第 4 条修复的单次 `\r`(2000ms 后发一次)只能带过第①道(默认选项恰好是「是」);第②道从未被处理——**如果不干预,会话就停在这里,不会自动前进也不会自动退出**(与第 4 条描述的「停在等 Enter」现象一致,只是停留的位置往前挪了一道)。人工介入实验(手动经 `PtyInput::Bytes` 发送方向键+Enter 选中「是,接受」)证实可以推过第②道、真正进入聊天主界面(终端标题变为 claude 的窗口标题,说明已进入正常会话),但**紧接着该次真实进程很快自行退出,没有产出任何回复内容**,jsonl 依旧不存在——第三层原因未查明(时间/范围所限未继续深挖:可能是 claude 自身在真正开始处理请求前还有别的检查、也可能是探针实验残留按键序列造成的干扰,两种可能都还没有证据区分)。
   - **来源**:`pty_backend::unix::UnixPtyBackend::run` 目前的 `submit_delay` 机制只发一次固定的 `\r`,不识别当前屏幕上是哪一种画面,对「全新工作区需要先过至少两道确认」这件事完全不知情——这不是第 3/4 条里已经描述过的问题,是两者都修完之后才第一次看见的、更深一层的阻塞。
   - **待办**:这不是「再加一次 `\r`」能简单解决的——第②道对话框的默认选项是危险方向(选错等于话都没说完就把会话退出了),需要专门设计「怎么识别当前是哪个画面、该发哪个按键」的方案(候选方向包括:识别特定文本/转义序列后按需应答;或者调查 claude 是否有配置文件/环境变量可以预先标记工作区为「已信任」+已接受 bypass-permissions 提示,从而让这两道对话框直接不出现,更接近这条问题的根子而不是逐个应付它的 UI)。第三层「进入主界面后很快退出」的根因也需要先查清楚,才能判断修完前两道对话框是否真的等于跑通。这是一个需要专门设计取舍的独立任务,不该在发现它的这次改动里顺手改。
   - **补充**:这道「工作区信任」确认是否也会出现在 BW 生产环境的真实用法里(每个 issue 一个新 worktree,`bw/issue-N`),还是只在这类每次都从零新建临时目录的验证脚本里才会触发(比如 claude 的信任判定可能是按项目根 / git 远程身份而不是按精确的 worktree 子目录路径),这一点本条也没有查清楚,留给下一个会话确认。
   - **登记日**:2026-08-10。

7. **生产路径上的工作区供给尚未落点**(登记日 2026-08-10,切片四A 实施时按 design-s4-runmanager.md §10 附带缺口登记)
   - **现象**:运行管理器(下一任务)要求调用方给一个已存在、且分支已切好的工作区,自己不造(切片三定的边界:谁给的工作区谁负责)。真实使用时总得有人造 worktree,而这段代码在 v1 里直接调 git,编排层(`bw-app`)被 `scripts/guard-no-direct-process.sh` 禁止这么做。
   - **来源**:切片二裁决 #1 把 git 辅助放进连接器 crate 当「内建函数」(注明不是连接器),命名将就问题当时就留着了(切片三开放问题 4)。
   - **待办**:切片五正面撞上时一并解决——抽出来、定名、定落点。
   - **登记日**:2026-08-10。

8. **五竞态验收(`run_races` 指挥器)与「真实并行三件」档尚未跑**(登记日 2026-08-10,切片四A 实施时按 design-s4-runmanager.md §10 附带缺口登记,条件按实际进度改写)
   - **现象**:本条设计稿原文按「若切片四合入时三-2 尚未落地」立条件——但复核工作区实况(`git log --oneline -3` 含 `2feb2ce`),三-2(agentcli 层注册表 + claude 执行连接器)与三-2 修复轮、缺口清偿轮**均已落地**,条件本身已不成立。真实的现状是:切片四A(本次)只搭了 `bw-store`/`bw-app` 骨架与两把结算/关门守卫(`store_guards` 指挥器已证),运行管理器本体(开工/取消/结束回写/重启清理)与 `run_races` 指挥器(十件并行 + 五竞态)按 design §10 的切分留给下一任务——「真实并行三件」(`run_races --real`)因此不是被三-2 卡住,而是单纯还没到实现它的那个任务。
   - **来源**:切片四内部的 commit 切分(design §8 建议 A-E 五个 commit,本次只完成相当于 A 的部分)。
   - **待办**:下一任务把 `run_races` 指挥器(含 mock 档的十件并行 + 五竞态)实现并跑绿之后,再跑一次 `run_races --real`(依赖三-2,现已具备前置条件),证据存本机 `verification/`,在本表补记。
   - **登记日**:2026-08-10。**mock 档已消灭于切片四D commit `062e75a`**(`run_races` 十件并行 + 五竞态全部确定性复现,169 条断言,`RUN_RACES_OK`,不依赖网关,已进常绿门禁)。**「真实并行三件」这一半仍未消灭**:切片四E 按待办跑了 `run_races --real --n 3`(前置条件三-2 已具备,claude 在 PATH),两次实跑结果一致——3 件全部 `start` 成功、拿到 `upstream_session`,但 90 秒轮询窗口内全部停在 `Running`,读回 `~/.claude/projects/<encoded>/<upstream>.jsonl` 三件均不存在;如实登记为**受阻**,阻塞原因就是下方第 6 条(全新工作区首次交互式启动的双重确认对话框),不是新阻塞,是第 6 条在这一档场景下的又一次复现。证据留本机(不入仓)。

9. **「待人处理」只证明了数据形状,投影未建**(登记日 2026-08-10,切片四A 实施时按 design-s4-runmanager.md §10 附带缺口登记)
   - **现象**:遗留运行(关了门但没结账,`ended_at IS NOT NULL AND settled_at IS NULL`)这个形状在新 schema 下能用一条 SELECT 查出来(`run` 表 `ended_at`/`settled_at` 两列独立可空,design §2.5),但没有任何代码把它变成一个清单或投影;本次的 `store_guards` 指挥器也没有专门跑这条 SELECT(它验的是两把守卫本身,不是这条派生查询)。
   - **来源**:本片刻意的范围裁剪(投影是视图,提前建就是给切片五留一处要改的地方)。
   - **待办**:`run_races` 指挥器(下一任务)按 design §5.2 第 9 条把这条 SELECT 纳入读回清单;真正的「待人处理」列表 UI 留给切片五建。
   - **登记日**:2026-08-10。**已消灭于 next 切片五D commit `4d82942`**:五项投影(`bw-app/src/view/attention.rs` + `App::attention_view`)全部落地,`hex_readback` 指挥器逐项正反验证(造出来→出现,消掉→自然消失),UI 列表本身仍是切片五E(桌面壳)的事,但「投影未建」这条缺口本身已经清偿。

10. **`CREATE INDEX IF NOT EXISTS` 对存量库不更新索引定义——迁移双守卫只管列、不管索引**(登记日 2026-08-10,切片四-1 复审 Important-2)
    - **现象**:`sqlite3` 独立复现:同名索引已存在时,一条谓词不同的 `CREATE UNIQUE INDEX IF NOT EXISTS` 会被静默忽略,`sqlite_master.sql` 里留的还是旧定义。这与本仓库「`CREATE TABLE IF NOT EXISTS` 对存量表不加新列」是同一类坑,只是从列扩到了索引;现有的迁移双守卫(`add_column_if_missing`,`next/crates/bw-store/src/sqlite.rs`)只覆盖列,没有对应机制覆盖索引定义。
    - **来源**:`next/crates/bw-store/src/sqlite.rs`(开库流程,`CREATE UNIQUE INDEX IF NOT EXISTS uq_run_live_delivery_per_issue` 由这里逐语句执行);评审复现记录见切片四-1 独立复审全文 Important-2。
    - **待办**:将来一旦要修 `uq_run_live_delivery_per_issue`(或本片之后新增的任何索引)的谓词,需要先加一条索引迁移(开库时比对 `sqlite_master.sql` 与期望定义,不一致就 `DROP INDEX` 后重建),不能只改 `schema.sql`——否则存量用户库永远修不上,新库老库行为分叉。本次(切片四-1 修复轮)只在 `schema.sql` 索引旁补了如实注释与本条登记,不修机制,机制留给真正需要改这条谓词的那次任务。
    - **登记日**:2026-08-10。

11. **「同工作区串线校验」只在单进程存活期内成立,重启后失效**(登记日 2026-08-10,切片四D 实施时发现)
    - **现象**:主控裁决 #5 要求的「同一个工作区已经有活跃运行时,第二次开工如实拒绝」(`RunError::WorkspaceBusy`)靠 `RunManager` 循环任务内存里的 `by_workspace: HashMap<PathBuf, RunId>` 实现——设计本身就把它定成「单点实现,不跨层」(不查数据库,不要求跨重启)。这意味着:进程重启后 `by_workspace` 从空表开始,如果 `reap_on_restart()` 还没被调用(该方法本身也不是自动触发的,design §3.1「不自动清理遗留」),此时对一个「库里还开着旧运行、但本进程从未见过」的工作区发起 `start()`,`by_workspace` 查不到冲突、`create_run` 也不会因为工作区重复而报错(唯一索引只按 `issue_id` 建,不按 `workspace`)——两个不同的 issue 若被指到同一个工作区,新的一个能顺利插行开工,之后 agentcli 层的会话续接会按工作区路径认出旧会话、把新活的开局接到旧活的历史对话里,这个错位不会被任何一层挡住。
    - **来源**:`next/crates/bw-app/src/run/manager.rs`(`Loop::by_workspace` 字段与 `handle_start` 里的检查),按 design-s4-runmanager.md §11 开放问题 5 与主控裁决 #5 的字面范围实现——裁决本身只要求「单点实现」,没有要求跨重启;`run_races` 指挥器的同工作区串线校验一节(`section_workspace_guard`)验证的正是「单进程内」这半句,没有覆盖重启后的这条空窗,如实标注不假装测过。
    - **待办**:真要补上跨重启这一半,两个方向:①在 `reap_on_restart()` 里顺带扫一遍数据库里所有还开着的运行、把 `by_workspace` 重建起来(需要 `run` 表加一条按 `workspace` 分组的查询,或者把重建塞进 `RunManager::open()` 本身而不是等显式调 `reap_on_restart`);②或者接受这条校验就是「尽力而为,不承诺跨重启」的定位,把这句话写进 `RunManager::start` 的文档注释里(目前文档没有提到这个边界)。哪个方向对,以及要不要现在就补,留给切片五接手时按真实撞上的场景定。
    - **登记日**:2026-08-10。

12. **`honest_close_on_storage_error`(以及 `handle_cancel`「库里开着、不在内存」分支里的补写关门路径)缺故障注入,存储调用真出错这条分支仍是零覆盖**(登记日 2026-08-11,切片四-2 修二实施时发现,来源:独立复审 `task-s4b-review.md`「修复复审」新发现 1)
    - **现象**:切片四-2 修 A(commit `f274bac`)新增的 `Loop::honest_close_on_storage_error`(三处调用点:`handle_start` 的回滚 / `handle_started` 的 `Ok(false)`/`Err` 分支 / `handle_observed` 的 `Err` 分支)要触发,前提是**存储调用本身出错**——`run_races` 指挥器里的存储层是真实 `SqliteStore`,正常路径下 `close_run`/`settle_run`/`mark_issue_in_progress` 不会自己报错,没有任何断言能走到这几条分支。复审用 panic 探针实测过:在 `honest_close_on_storage_error` 函数体第一行插一句 `panic!`,复跑 192 条断言全过、一次都没 panic——证明这条诚实收尾路径(评审 Important-1 的产品代码修复)修完之后仍然是零覆盖,任何回归都不会被这份指挥器发现,也造不出能让它变红的突变。`handle_cancel`「库里开着、不在内存」分支里真正调用 `close_run` 的那部分,本轮(切片四-2 修二,commit 见本条登记日当天的 next 切片四-2 修二 commit)已经用 R5 新增的 7 条断言补上覆盖(`run_races.rs` `section_r5`,reap 之前对遗留运行调一次 `cancel`),**不再计入本条缺口**;本条现在只剩 `honest_close_on_storage_error` 本体。
    - **来源**:`next/crates/bw-app/src/run/manager.rs` `Loop::honest_close_on_storage_error`(定义于 commit `f274bac`);复审判定见 `task-s4b-review.md`「修复复审」新发现 1 与 concern 4。
    - **待办**:需要一个能按调用次序人为报错的 store 包装做故障注入(比如包一层 `SqliteStore`,「第 N 次调用某方法就返回 `Err`」),让 `run_races` 能确定性地让 `close_run`/`mark_issue_in_progress` 在正确的时机报错,从而真的走一次这条分支,断言其收尾效果(内存三张表槽位真的释放、库里那一行如实标成失败或者保持 `ended_at=NULL` 等待下次 `reap_on_restart`)。这个故障注入机制不是本轮范围,留给下一个专门做它的任务。
    - **登记日**:2026-08-11。

13. **R6「取消后晚到的完成消息」经真实轮询链路造不出来,是设计层面的结构性局限,不是实现疏漏**(登记日 2026-08-11,切片四-2 修二实施时记账,复述独立复审对 Critical-2 的根因辨析,不改实现)
    - **现象**:design-s4-runmanager.md §4.1 给 mock 的四行 `poll` 逻辑第一行是「被取消过 → 恒报 `Canceled`」;§3.5②/主控裁决 #6 又要求 `RunManager` 取消时打断轮询任务(`poll_cancel.cancel()` 一响,轮询任务在下一次 `select!` 立刻退出,不会再发第二条消息)。这两条设计合在一起,§4.2 R6 原文那条「取消后 300ms 才到的完成消息」——完整走轮询任务真实投递的那种晚到消息——在这份指挥器里**结构性造不出来**。`next/crates/bw-app/examples/run_races.rs` `section_r6` 现在改成直接对存储层重放一次 `close_run`(绕过 `RunManager`,`【模拟晚到】` 自我标注),把「关门只发生一次」这把守卫从零覆盖抬到有覆盖,但这条重放**不经过**轮询任务/取消令牌那条真实链路——§4.3「晚到消息的钥匙 = 运行编号」的完整链路仍然没有被证过。
    - **来源**:独立复审 `task-s4b-review.md` `774c3cb..647bba6` 区间「修复复审」Critical-2 判定与 concern 1(该复审已认定这是「如实、合理,但需要一个记账动作」);代码注释见 `run_races.rs` `section_r6` 里紧邻 `close_run` 重放调用之前的说明段。
    - **待办**:不是重做,是记账——将来读 design §4.2 R6 原文的人应该先看到这条登记,而不是误以为完整轮询链路验过。真要补全,需要按复审给的两条路之一动 mock:①给「取消后仍按原脚本报 `Finished`」开一个脚本开关(仅 R6 那条分支打开,自我标注),让晚到的完成消息真的经轮询任务送回;②或者接受当前的「直接重放 `close_run`」已经是最便宜、够用的覆盖方式,只补齐这条记账即可,不必再动 mock。哪个方向对,留给下一个真正需要验证完整轮询链路的任务定。
    - **登记日**:2026-08-11。

14. **工作树只造不删,会越积越多**(登记日 2026-08-11,切片五A 实施时按 design-s5-hexpanel.md §12 第 3 条附带登记)
    - **现象**:`next/crates/bw-workspace/src/provision.rs` 的 `provision_issue_worktree` 从 v1 搬过来时,刻意**不搬** v1 那个「作用域结束自动删」的 `IssueWorktreeGuard`(`Drop` 里强制 `git worktree remove`)。因此每件活开工造出来的工作树目录不会自动消失,长期运行下工作树会越积越多,占用磁盘。
    - **来源**:主控裁决(design-s5-hexpanel.md 附「主控裁决」#5 附带确认)钉死的语义:上游按工作目录编码存会话,续接必须同路径(切片三验过的硬事实);一次交付运行结束后,它的会话很可能还活着(「降级为咨询」就是专门为这种情况准备的:名额放开、会话不杀、工作区不清)。自动删工作树 = 让那个还活着的会话再也接不回来——这是有意的取舍,不是遗漏。
    - **待办**:做一条显式的清理用例,判据必须包含「这个路径上没有活着的上游会话」,不能无条件删;在此之前只能靠人手动清理或接受磁盘占用增长。
    - **登记日**:2026-08-11。

15. **交棒记录的「带险时缺了什么」仍只是自由文本注记,没有结构化欠账明细**(登记日 2026-08-11,切片五B 实施时按 design-s5-hexpanel.md §12 第 2 条附带登记)
    - **现象**:`next/crates/bw-store/src/schema.sql` 新建的 `handoff` 表字段与索引逐字移植 v1 同名表——只有一个自由文本的 `note` 列,带险交棒时「完成清单没勾完的那部分具体是什么」只能靠人手写进这个字段,没有结构化的欠账明细(比如「哪几项 DoD 没勾」这种可查询、可统计的形态)。`CLAUDE.md` 要求带险交棒「永久记下当时缺了什么」,目前只在文本层面满足,不在结构层面满足。
    - **来源**:结构化欠账明细的前提是先有「完成清单勾没勾」的状态表,而勾选状态今天没有写入方——六段总控是只读屏,勾选入口本片(及切片五C/D)都不做(design §8「明确不做」清单第 6 项)。v1 本身也只有注记列,不是这次移植退化的。
    - **待办**:做勾选入口的那一片(尚未排期)一并建结构化状态表,并把交棒表的欠账明细从自由文本升级成结构化引用。
    - **登记日**:2026-08-11。

16. **`issue.stage` 今天没有任何生产写入方**(登记日 2026-08-11,切片五C/D 实施时发现)
    - **现象**:`issue.stage`(五B 加的列,design §2.1)有真实读侧消费者——六段总控②「五角色责任卡」的活数分组(`IssueStore::count_issues_by_stage`)——但没有任何命令/用例会把一件活的 `stage` 从 `NULL` 推到某个值。`hex_readback` 指挥器为了让这一段有真数据可读回,绕过 store 层用独立连接直接 `UPDATE issue SET stage = ?`,每一行都标注「非生产路径产出」(design §7.4 降级口径的同一惯例)。查询机制本身是真的(读回 `Some(Build)` 与直接写入的值一致),但生产路径今天走不到这个值。
    - **来源**:design-s5-hexpanel.md 的命令面草案(§4.2)与本片的范围裁剪(§8)都没有点名一条「把活分到某个阶段」的命令——切阶段(`SetActiveStage`/`HandoffStage`)动的是 `project.active_stage`,不是逐件活的 `issue.stage`,两者是两回事,没有一条命令负责后者。
    - **待办**:补一条命令(例如 `SetIssueStage { issue, stage }`)与对应的 `cmd::issue` 用例,让这一列有真实写入方;在此之前,五角色责任卡的活数分组在生产环境里会一直是「全部未归类」。
    - **登记日**:2026-08-11。**2026-08-11 复审裁定**(task-s5b-review.md §6 concern 1 台账口径收紧):待办口径从开放式「补一条命令」收紧为——**五E(桌面壳)立壳之前必须补上这条生产写入方**,不能带着「五角色责任卡活数分组在生产环境永远显示全部未归类」这个已知空缺进入桌面壳阶段。**已消灭于 next 切片五E commit `9c2dfcc`**:`Command::SetIssueStage { issue, stage: Option<StageKind> }` + `cmd::issue::set_stage` + `bw_store::IssueStore::set_issue_stage` 三层落地,`examples/seed_shell_demo.rs` 真实经这条命令总线给一件活写 `stage`,深链 sqlite 独立读回验证生产路径确实写入(`issue.stage=2` 对应 `Build`),不再是 `hex_readback` 那种绕过 store 的非生产写法。

17. **待人处理④b「数据过期」判断,全体指标共用同一个硬编码节奏窗口(Weekly)**(登记日 2026-08-11,切片五D 实施时按 design §3.2④b 附带登记)
    - **现象**:`bw-app/src/view/hex.rs` 的 `DEFAULT_METRIC_CADENCE`(`Cadence::Weekly`)是全体指标共用的唯一默认值,喂给 `bw_core::derive::measure`/`cadence_window` 判断一条观测是不是「过期」。`metric` 表(design §2.2)没有「这条指标多久刷新一次」的列,`.bw/metrics.toml` 的格式(`docs/metrics-toml-format.md`)也没有约定这个字段——按指标定制节奏今天做不到。
    - **来源**:本片刻意的简化(设计稿 §3.2④b 只要求「过期判断留内核」,没有钉死「每条指标能不能定制节奏」这条产品口径)。
    - **待办**:真要按指标定制节奏,需要先给 `.bw/metrics.toml` 加字段(比如 `cadence = "daily"`)、`metric` 表跟着加列(走迁移双守卫),同步器与派生链调用点跟着改——这是下一片的事,不在本片范围。
    - **登记日**:2026-08-11。

18. **字体没有真的随包打包,退回系统字体栈**(登记日 2026-08-11,切片五E 实施时按 design §4.5 附带登记)
    - **现象**:design-s5-hexpanel.md §4.5「壳的杂项」明确要求把 Noto Serif/Sans SC + JetBrains Mono 三套字体随包打进 `app-desktop`(离线正确性 + 中文整形在这套 WebView 上要实测)。`next/crates/app-desktop/src/theme.rs` 目前和仓根旧壳一样,只给了系统 CJK 字体栈兜底(Songti/PingFang · SimSun/微软雅黑),没有真的 `asset!()` 打包任何字体二进制文件。
    - **来源**:这个仓库里没有任何字体二进制文件可用(`find` 过全仓,零命中),本片的执行环境也没有可靠、经许可的路径去获取字体文件并核实授权——不臆造一份没有真实来源的字体资源,比"假装打包了"更诚实。仓根旧壳从建起来就是同样的状态,不是本片新引入的退化。
    - **待办**:拿到经授权的字体二进制文件(比如从项目仓另外的位置引入,或用户提供)后,用 `asset!()` 真的打进 `app-desktop`,替换掉系统字体栈兜底;顺带验证中文整形在这套 wry WebView 上的真实渲染效果。
    - **登记日**:2026-08-11。

19. **逃生舱在真实深链渲染里展示不出「活着的」进行中运行,要等 RunIssue 接入壳**(登记日 2026-08-11,切片五E 实施时深链验证撞见)
    - **现象**:`RunManager::reap_on_restart()`(design-s4-runmanager.md §3.1)在 `bw-next` **每次**启动时都会真的调用一次,把「数据库里还开着、但这个新起的进程没有活跃句柄」的运行如实标成 orphaned 并结账。这意味着任何不是由**当前这个存活进程**的 `RunManager::start()` 开工的运行,只要经历过一次 `bw-next` 重启,就再也不会出现在「当前 Loop」段的进行中卡片或逃生舱里——`examples/seed_shell_demo.rs` 造的那个「进行中」运行行,在紧随其后的 `BW_OPEN` 深链启动里被立刻 reap 成了 orphaned(真实观测到:第一次深链启动 stderr 打出 `[BW_REAP] 重启收拾遗留运行 1 条`)。
    - **来源**:这是**正确的产品行为**,不是缺陷——一次运行不该在没有任何活着的进程盯着它的情况下继续「看起来在跑」,这正是 `reap_on_restart` 存在的意义。但它的副作用是:本片(切片五E)没有实现 `RunIssue`(壳不发起开工,design §3.3/§8 明令),所以**没有任何路径能让一个运行的完整生命周期(开工到关门)发生在同一个存活的 `bw-next` 进程里**——这正是逃生舱唯一能在深链渲染里展示"进行中"卡片的前提。本片对逃生舱"必须真能用"(裁决 7)的举证因此改走另一条路:`escape_hatch::build` 这个纯函数直接喂一条真实写入 `run` 表的行,证明它算出的续接命令语法正确(seed_shell_demo.rs 的做法);"重启后这行还会显示为进行中"这句话本片不敢声称,如实不声称。
    - **待办**:等 `RunIssue` 接入壳(design §8 范围裁剪之外的下一片)之后,一次真实点开工的运行会在同一个存活进程内经历完整生命周期,那时才有条件在深链渲染里真实展示一张「进行中」的逃生舱卡片。
    - **登记日**:2026-08-11。

20. **`BW_WORKSPACES` 深链变量本片未消费**(登记日 2026-08-11,切片五E 实施时如实登记)
    - **现象**:design-s5-hexpanel.md §4.4 深链变量表把 `BW_WORKSPACES`(工作区根目录)列为「不变」——四个深链变量之一,`app-desktop` 目前只实现了 `BW_DB`/`BW_OPEN`/`BW_PANEL` 三个,没有读取或使用 `BW_WORKSPACES`。
    - **来源**:本片没有任何用例需要「工作区根目录」这个概念——`RunIssue`(会用到它来给新开工的活分配工作区)没有接入壳(design §3.3/§8 范围裁剪),交付证据第③栏的工作区现采直接读 `project.root_path`(单个项目自己的检出根,不是一个「根目录」概念)。为一个没有真实消费者的环境变量写解析代码,会立刻在门禁里显出一处死代码,不如如实不实现,登记留痕。
    - **待办**:`RunIssue` 接入壳、需要给新开工的活自动供给工作区时,一并接上 `BW_WORKSPACES`(同 `bw-workspace::provision_issue_worktree` 的既有能力对接)。
    - **登记日**:2026-08-11。
