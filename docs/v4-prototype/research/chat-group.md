# 项目群接口预研:让「WeLink 由同事实现、外部群随便换」不用碰 buddy 架构

> **30 秒导读**:这是一篇**预研**(不是设计定稿),给谁看——要接手「项目群」这块的人(包括写 WeLink 实现的同事)和后续做详细设计的人。**作数吗**:调研结论(buddy 现有连接器怎么做、各聊天工具群 API 长什么样)是核实过的事实,可以直接引用;§3 的接口形状是**建议**,还没拍板,写进 [`mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) 前需要再过一轮评审。背景与需求见母文档 §1 愿景③、§2.6、第 1 站、§6、§7、待拍-26,规范文件里 `[chat]` 段的雏形见 [`standard-module-draft.md`](../standard-module-draft.md) 第 1 类。看不懂的词先查 [`../../../CONTEXT.md`](../../../CONTEXT.md)。

---

## 一句话结论(三行人话)

1. buddy 今天已经有一套「同一件事、不同远端实现」的标准做法——把它原样搬过来定「项目群」的接口就够,不用另起炉灶。
2. 调研六家聊天工具发现一个硬事实:**发消息几乎家家都能做,拉历史很多家做不到**——群机器人多数是「只发不读」的单向通道,接口设计必须把「这个群没有拉历史的能力」当成一种诚实的正常状态,而不是错误。
3. WeLink 的公开文档只写到「有群组消息这回事」,没写到参数级——这条不影响接口设计(同事按 §5 的骨架实现就行),但意味着「探活」「历史窗口大小」这类细节要等同事对着真实 WeLink 环境核实,不能靠今天查到的公开资料拍死。

---

## 1 · 事实:buddy 今天的连接器长什么样

读的文件:`crates/bw-engine/src/remote.rs`、`codehub.rs`、`github.rs`、`connectors_file.rs`,以及 `crates/bw-app/src/` 里调用它们的地方(`dispatch.rs`、`project_sync.rs`、`terminal.rs`、`lib.rs`)。

**codehub / GitHub 怎么被区分与选择**:是一个瘦 enum + 工厂函数,不是 trait。`remote.rs` 定义:

```rust
pub enum Remote {
    Github(String),                        // "owner/repo"
    Codehub { host: String, path: String }, // host = 绿/黄/内源三选一,path = "org/repo"
}

impl Remote {
    pub fn for_project(provider: &str, host: &str, path: &str) -> Result<Self, RemoteError> {
        match provider.trim() {
            "github" | "" => Ok(Remote::Github(path.to_string())), // "" 是老库兼容
            "codehub" => Ok(Remote::Codehub { host: host.trim().into(), path: path.into() }),
            other => Err(RemoteError::UnknownProvider(other.to_string())),
        }
    }
    // probe / create_issue / create_mr / merge_mr / list_open_issues … 每个方法
    // 内部一次 match,分别转发给 crate::github::xxx 或 crate::codehub::xxx
}
```

关键设计判断,直接可搬:**project 表存 `provider`/`remote_host`/`remote_path` 三个字符串字段,`Remote::for_project` 在一处把它们翻译成 enum;调用点(`bw-app` 里几十处)只调 `remote.probe()`、`remote.create_issue()` 这类方法,永远不自己 `match` provider**。新增一个 provider 只改 `remote.rs` 一个文件,不用动调用点(`remote.rs:8-13` 的模块级文档原话就是这条纪律)。

**登录态放哪**:两家都不进 BW 自己的库或仓,而是各自 CLI 的本机凭证——GitHub 用 `gh auth login`(`gh` 自己的本机配置,`github.rs:1-3` 模块文档「Relies entirely on the user's own `gh auth login`」);CodeHub 用 `codehub-cli auth login` 写入 **OS keyring**(`codehub.rs:1-7`「token 存 OS keyring」),且每次调用都显式带 `-H <host>`,不靠猜哪个 host 有 token。两条路都是「shell-out 现成 CLI,BW 自己不摸 token」,与 `standard-module-draft.md` 定的「登录凭证在本机设置,不进仓不进库」是同一条纪律的两个实例。

**错误怎么报到 UI(如实一句话)**:`GithubError`/`CodehubError` 各自是一个小 enum(`NotInstalled`/`Command`/`Parse` 这几种形状),被 `RemoteError` 用 `#[error(transparent)]` 包一层,再被 `bw-app/src/lib.rs:99-103` 的 `impl From<RemoteError> for AppError` 收进已有的 `AppError::Engine(String)` 口径——就是把错误原文拼进一句字符串。**没有结构化错误码,最终落地就是一句人话文本**(toast 或活的重试提示),Issue 停在原状态可重试,不假装成功。这与全仓「失败就诚实停住」的纪律一致。

**旁证**:`connectors_file.rs` 是另一套已经在跑的模式——`.bw/connectors.toml` 正本进仓,一个专门的 `*_file.rs` 模块负责解析(`deny_unknown_fields`,格式错就整份拒收、不静默吞掉),读出来的是纯数据,「怎么用」交给调用方。这条「配置正本进仓 + 专门模块解析 + 拒绝静默」的模式,正是 `standard-module-draft.md` 里 `.bw/project.toml [chat]` 段打算走的路——不是我推荐,是仓里已经有先例。

## 2 · 事实:主流聊天工具的群 API 长什么样

**结论先说**:六家里,**发消息**都有官方通道;**拉历史**只有飞书(带权限门槛)和 Slack(标准能力)做到「群机器人式接入也能读」,钉钉/企业微信的「群机器人」和 Teams 的「Incoming Webhook」**都是单向只发**——要读历史得换一整套完全不同、门槛高得多的体系(企业微信的「会话内容存档」是合规监管功能,不是群机器人的一部分)。WeLink 公开文档没有查到能确认或排除的证据。

| 提供方 | 认证方式 | 群怎么标识 | 能否拉历史 | 消息格式 | 速率限制 | 出处 |
|---|---|---|---|---|---|---|
| **飞书(Lark)** | `tenant_access_token`(应用身份)或 `user_access_token`,放 `Authorization: Bearer <token>` | `chat_id`(`oc_` 开头) | **能**,但有权限门槛:光有单聊权限拿不到群消息,应用还必须额外申请「获取群组中所有消息」(`im:message.group_msg`)权限 | `msg_type` 支持 text/post(富文本)/image/file/interactive 等;`content` 是一段 JSON 字符串 | 同一群 5 QPS(群内机器人共享) | [发送消息](https://open.feishu.cn/document/server-docs/im-v1/message/create?lang=zh-CN)、[获取会话历史消息](https://open.feishu.cn/document/server-docs/im-v1/message/list?lang=zh-CN) |
| **钉钉(DingTalk)** | 自定义机器人:webhook URL 本身即凭证(URL 带 `access_token` 查询参数),可选叠加三种安全设置之一——自定义关键词(消息必须含关键词)、加签(HmacSHA256,`timestamp+"\n"+密钥` 签名)、IP 段白名单 | webhook URL 本身就是标识,没有独立群号暴露给调用方 | **未找到**——公开文档没有配套的「拉群历史消息」读接口,群机器人是纯发送通道(未核实是否存在其他非机器人路径) | text/markdown/link/actionCard/feedCard | 文档页面未查到明确数字(未核实) | [企业内部机器人 webhook](https://open.dingtalk.com/document/orgapp/assign-a-webhook-url-to-an-internal-chatbot)、[机器人回复/发送消息](https://open-dingtalk.github.io/developerpedia/docs/learn/bot/appbot/reply/) |
| **企业微信(WeCom)** | 群机器人(消息推送):在企业微信群里手动添加机器人拿一条 webhook URL,`key` 查询参数即凭证 | webhook URL 本身即标识(一个 webhook 对应一个群里的一个机器人实例) | **群机器人不能**;要拉历史得走完全独立的「会话内容存档」体系(企业级合规监管功能,需配置消息加密公钥+专用 SDK,公开页面未写明是否收费,只写明**只能拉 5 天内的记录、一次上限 1000 条、分页拉取、调用频率≤4000次/分钟**)——这套东西面向「企业统一监管员工沟通合规」,不是「某个项目群的机器人」,定位与 buddy 要的东西不同 | `msgtype` 支持 text/markdown/markdown_v2/image/news/file/voice/template_card | **20 条/分钟**(每个 webhook) | [消息推送配置](https://developer.work.weixin.qq.com/document/path/99110)、[会话内容存档概述](https://developer.work.weixin.qq.com/document/path/91360) |
| **Slack** | Bot token;发消息需 `chat:write` scope,读历史需 `channels:history`/`groups:history`/`im:history`/`mpim:history` 之一(按会话类型) | `channel` ID(如 `C0123...`) | **能**,标准能力:`conversations.history`,`oldest`/`latest`(unix 秒)时间窗 + `cursor` 游标分页 + `limit`(默认100最大999);每条消息含 `user`、`text`、`ts` | `chat.postMessage`,`text` 或 `blocks` 富文本,默认 URL 自动转超链接 | 发消息:同频道约 1 条/秒,workspace 级几百条/分钟;读历史:内部 App Tier 3(约 50+/分钟),**对外分发的商业 App 被限到 1 请求/分钟 + `limit` 强降到 15**(内部工具不受此条限制,记一句备查) | [chat.postMessage](https://docs.slack.dev/reference/methods/chat.postMessage)、[conversations.history](https://docs.slack.dev/reference/methods/conversations.history/) |
| **Microsoft Teams** | Incoming Webhook 正在被官方**逐步废弃**(Microsoft 365 Connectors 即将停止新建),现在走 Teams 内「Workflows」App 模板(如「Send webhook alerts to a channel」)生成一条 webhook URL,本质仍是「一条密钥 URL 即凭证」 | webhook URL 本身即标识,绑定到具体频道 | **没有**——Incoming Webhook 是纯单向发送通道,官方文档没有配套读历史 API;真要读 Teams 消息历史得走 Microsoft Graph API + Bot Framework + 应用权限/管理员同意,门槛高出一个量级,本次未深入(未核实细节) | JSON payload(`{"text":"..."}`)或完整 Adaptive Card;消息大小上限 28KB | 同一 webhook 每秒 4 次以上会被 429 限流,官方建议指数退避重试 | [Create an Incoming Webhook](https://learn.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/how-to/add-incoming-webhook) |
| **WeLink(华为)** | 未核实到参数级——概述页只写「群组消息:应用以系统身份发文本/卡片消息」,没有展开 access_token 怎么换、群标识具体形态 | 未核实(概述页未展开) | 未核实(概述页未找到历史接口) | 未核实(概述页未展开消息体格式) | 小微推送(另一种消息类型,不是群组消息)**每企业每天限 100 次**;群组消息本身速率未核实 | [消息通知概述](https://support.huaweicloud.com/devg-welink/start-13.html) |

对 WeLink 的判断:公开文档确实只到「有群组消息这回事」的程度,再深入的参数需要同事对着真实 WeLink 开放平台账号才能核实——这与背景要求一致(「WeLink 内部收发函数的实现不归我们负责」),buddy 侧不需要吃透这些参数,只需要§3 的接口形状能装下它。

## 3 · 接口形状:两个必选函数 + 一个可选

**人话**:抽象层面,项目群这件事只需要两件事——「往群里说一句话」和「回头看看群里说了什么」;第三件「群号填得对不对」是锦上添花。调研结果里最扎眼的一条是**拉历史这件事很多群机器人天生做不到**(钉钉/企业微信/Teams 三家的「机器人」都是只发不读),所以接口不能假设「配了群就能拉历史」——「这个 provider 老实说做不到拉历史」必须是一个**正常返回值**,不是异常,调用它的运作活①要能安静跳过,而不是报错卡住。

**为什么用 trait + 工厂函数,而不是照抄 `Remote` 的 enum**:`Remote::Github`/`Remote::Codehub` 两个变体的字段形状几乎一样(`host`+`path`),适合直接塞进一个 enum。项目群的提供方天生更杂——WeLink 用群号,外部候选(Slack/飞书/……)可能用 channel ID 或 chat_id,认证方式也各不相同,而且第一版就有「未配置」「本机自测」两个非真实提供方要装。用 `Box<dyn ChatGroup>` + 工厂函数换来的是:新增一个提供方只加一个实现文件 + 工厂里加一行 `match`,不用改 trait 定义,和 `Remote` 的核心思想(**调用点只调方法、provider 分支只在工厂这一处**)其实是一回事,只是换了个更适合异构提供方的容器。

```rust
// 工程对照(伪码,未拍板):项目群 = 一个 trait + 一个按 provider 名字造实现的工厂,
// 仿 bw_engine::remote::Remote「调用点只调方法、provider 分支只在一处」的做法。

/// 一条要发或读到的群消息。发消息时只填 text/link/markdown;拉历史时
/// 额外填 time/sender——两种用途共用一个类型,免得再定义一对几乎重复的结构体。
pub struct ChatMessage {
    pub time: Option<OffsetDateTime>,     // 拉历史时必填;发消息时留空,由群侧盖时间戳
    pub sender: Option<String>,           // 拉历史时的发言人昵称(可脱敏);发消息时忽略
    pub text: String,                     // 纯文本主体——所有提供方都必须能处理这一份
    pub link: Option<(String, String)>,   // (标题, URL)——回工作台或 MR 的可点链接
    pub markdown: Option<String>,         // 可选富文本正文;提供方不支持就退化用 text 拼
}

pub enum ChatError {
    NotConfigured,       // provider = "none",项目还没配群——如实,不是失败
    HistoryUnsupported,  // provider 老实做不到拉历史(钉钉/企业微信群机器人/Teams webhook 这类)
    Auth(String),        // 本机未登录 / 凭证过期,原文带上
    Network(String),     // 请求失败,原文带上,绝不吞掉伪装成功
}

pub trait ChatGroup: Send + Sync {
    /// 往这个群发一条消息。text 是所有提供方共同的兜底;link/markdown 是
    /// 可选增强,提供方按自己的能力挑一种用,用不了就退化成纯文本拼接——
    /// 绝不因为「这条我发不出富文本」整条消息发送失败。
    fn send(&self, msg: &ChatMessage) -> BoxFuture<'_, Result<(), ChatError>>;

    /// 拉 [since, until) 这段时间窗的历史,分页由实现自己吃掉(MVP 只用于
    /// 「上周一 → 本周一」这类小窗口,调用方不需要感知游标)。
    /// `Err(ChatError::HistoryUnsupported)` 是「这个 provider 天生没有这个
    /// 能力」的专属分支——调用方据此**跳过**,不当错误处理、不重试。
    fn fetch_history(
        &self,
        since: OffsetDateTime,
        until: OffsetDateTime,
    ) -> BoxFuture<'_, Result<Vec<ChatMessage>, ChatError>>;

    /// 可选:验证群号/凭证有效(配置页「测一下」按钮用)。默认实现可以直接
    /// 拿最近一个极小窗口跑一次 fetch_history 顶替,具体提供方也可以更便宜地
    /// 实现(比如探一次健康端点)。
    fn probe(&self) -> BoxFuture<'_, Result<String, ChatError>> {
        Box::pin(async { Err(ChatError::NotConfigured) }) // 默认「没实现就说没配」,不 panic
    }
}

/// 工厂:按 `.bw/project.toml [chat] provider` 字段造实现。未知 provider 一律
/// 退化成 `none`(如实显示「未配」),不 panic、不静默假装某个已知提供方。
pub fn for_project(provider: &str, group_id: &str) -> Box<dyn ChatGroup> {
    match provider.trim() {
        "welink" => Box::new(welink::WelinkChatGroup::new(group_id)), // 同事实现
        "mock" => Box::new(mock::MockChatGroup::new(group_id)),       // 本机自测,见 §5
        _ => Box::new(none::NoneChatGroup),                           // "" / "none" / 未知
    }
}
```

配置分两层(与母文档 §6「谨慎数据库」三层信息架构一致):**群号 / 提供方 / 同步哪些事**在项目正本 `.bw/project.toml` 的 `[chat]` 段(`standard-module-draft.md` 已定形状:`provider = "welink"`、`group_id = "..."`、`notify = ["review","merged","release"]`);**登录凭证**在本机设置里(不进仓、不进库,呼应 §1 GitHub/CodeHub 的先例)。发过什么记 `chat_outbox`(§4 展开)。

## 4 · 两个用途的具体流程

### ① 通知同步

- **触发的事件(默认三类,可勾)**:评审中(活推到 InReview)、已合入(MR/PR 合入)、发版(第 5 站发版本动作)——对应 `.bw/project.toml [chat] notify` 数组的三个取值 `review`/`merged`/`release`。
- **消息文案模板(一行)**:`【<事件>】#<活号> <活标题> · <MR/PR 号与状态> · <谁该动> → <工作台深链>`。例如:`【评审中】#42 实现登录页 · MR !17 待合入 · 该 committer 处理 → bw://open?issue=42`。用 `ChatMessage.text` 兜底,能发 markdown 的提供方(飞书 post / 企业微信 markdown)额外填 `link`,把「工作台深链」做成可点。
- **失败重试与去重**:靠 `chat_outbox`(§6 已在母文档数据模型增量里登记)。发之前先查这张表有没有 `(issue_id, event_type)` 的成功行——有就跳过,绝不重发;发送时先写一行「尝试中」,成功回填状态,失败保留「失败」状态等下次 scheduler tick 重试(具体退避节律留开放问题)。**成功的行永远不重发**——这是把issue 记账那条「同一件事绝不记两次」的纪律原样搬到通知场景。

### ② 运作活①的群摘要

- **拉取窗口**:「上周一 00:00 → 本周一 00:00」,固定按 ISO 周对齐,与 `docs/plan/YYYY-Www.md` 的周边界一致。
- **调用**:`fetch_history(since, until)`;拿到 `Err(HistoryUnsupported)` 或 `Err(NotConfigured)` 都**安静跳过**,不阻塞运作活①的其余步骤,agent 该干嘛干嘛,只是少一份参考。
- **本机摘要文件格式建议**:按天分组、每条一行(`HH:MM <发言人> <文本>`),**去掉表情与图片**(纯文本化,图片/文件类消息只留一句「[图片]」占位或整条丢弃——具体规则留详细设计),**长度设上限**(建议按字符数或行数截断,具体数字留开放问题,原则是「宁可截断也别让这份参考件比周计划本身还长」)。
- **住哪**:本机文件(§6 三层信息架构的第二层「本机文件,不进仓」),运作活①用完可删,不进仓、不进库——与 claude/Cursor 路径、终端会话缓存同一层。
- **喂给谁**:作为 §2.6 渐进加载「第 4 层项目知识」旁的参考件,和 `docs/plan/`、`.bw/metrics.toml`、codegraph 索引一起在 agent 开工前注入。
- **隐私与边界(重申母文档已写的红线)**:群里的话只做参考,**不做数据点、不点灯**——健康信号只能从真实观测推导,群消息摘要不是观测;发言人字段可脱敏为昵称,不落库、不进仓,用完即焚。

## 5 · 给同事的对接说明骨架(WeLink 实现者要知道的)

- **要实现的函数**:§3 trait 的 `send`(必须)、`fetch_history`(尽力,做不到就稳定返回 `Err(ChatError::HistoryUnsupported)`,不要抛异常或 panic)、`probe`(可选,建议实现,配置页「测一下」按钮体验会好很多)。
- **输入输出**:严格照 §3 的 `ChatMessage`/`ChatError` 类型走;`send` 只需要处理 `text` 一定有值,`link`/`markdown` 是「有更好、没有就退化」;`fetch_history` 返回的每条 `ChatMessage` 至少要填 `time`+`text`,`sender` 尽量填(方便复盘时看出是谁在讨论)。
- **错误约定**:凡是「这条路走不通」都必须如实归到 `ChatError` 的某个分支,**绝不吞掉错误伪装成功**(这是全仓「读回为证」纪律的自然延伸)——`Auth`/`Network` 两个分支把 WeLink SDK/HTTP 抛出的原始错误文本原样带上,不用重新组织措辞。
- **放哪个模块**:按母文档 §7「一个外部能力一个适配模块」的规矩,新建 `crates/bw-engine/src/chat/welink.rs`(与今天 `codehub.rs`/`github.rs` 平级的独立文件),配一份模块内 README 或顶部模块文档注释,记「借自哪里、怎么鉴权、已知限制」——参照 `codehub.rs` 顶部那段模块文档的写法。
- **怎么本机自测(不用真群也能测)**:两个占位实现已经在工厂里留好位——`none`(未配置,`send`/`fetch_history` 都直接返回对应的诚实错误,验证「没配群时别的流程不会崩」)与 `mock`(内存态假群:`send` 记进一个 `Vec`,`fetch_history` 返回调用方提前塞好的一批 `ChatMessage`,让运作活①的摘要生成逻辑、通知同步的去重逻辑都能在没有真实 WeLink 租户的情况下跑通单元测试与本机联调)。WeLink 侧的联调建议是:先对着 `mock` 把 buddy 这边的调用链跑通,再换真实 WeLink 凭证验证 `send`/`fetch_history` 本身。

## 推荐 / 不做什么

**推荐**:接口按 §3 定(trait + 按名字造实现的工厂),配置分两层(`.bw/project.toml [chat]` 存 provider/群号/同步事件,本机设置存凭证),记账靠 `chat_outbox` 做到「同一件事绝不重发」。

**MVP 不做**:
- **双向对话机器人**——群里 @buddy 发指令这种交互,超出「发通知 + 拉参考」的范围。
- **群里点按钮改活状态**——违反「完成永远人点、状态只能在工作台按钮上走」的产品铁律,不给群消息任何改变状态的权力。
- **多群**——一个项目一个群,不做多群路由。
- **正式实现除 WeLink 外的任何一家**——飞书/Slack/企业微信/钉钉/Teams 的调研只是为了抽公共接口,§3 的工厂位「外部待定」先占着,不预先选定、不预先写代码。
- **企业微信「会话内容存档」这类合规重型方案**——门槛高(专用 SDK、加密公钥、可能付费)且定位是企业合规监管,不是项目群场景,不作为拉历史的备选路径。

## 留给详细设计的开放问题(≤5)

1. **WeLink 群组消息的参数级细节**——access_token 怎么换、群标识具体是什么形态(群号/chat_id/别的)、`fetch_history` 到底能不能做——公开文档没查到,需要同事对着真实 WeLink 开放平台账号核实。
2. **外部提供方选哪家**——目前只留工厂位,不预先选型;等真有外部群需求时再定(候选参考 §2 的飞书/Slack,两家都验证过「能拉历史」)。
3. **摘要文件的具体篇幅上限**——字符数/行数截断的具体数字、「去掉表情图片」的具体处理规则(整条丢弃 or 占位替换),本次只给方向。
4. **通知失败重试的具体节律**——靠 scheduler 哪个 tick、退避几次、多久放弃改成「需要人手动重试」,留给实现时定。
5. **`chat_outbox` 去重键的粒度**——按 `(issue_id, event_type)` 够不够,还是要考虑「同一 issue 先后开过两个 MR」这类边界情况多加一维,留给数据模型详设。
