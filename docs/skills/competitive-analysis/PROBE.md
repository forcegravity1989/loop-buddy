# 检索探活记录(competitive-analysis,C10 · plan/13 D9)

竞品分析 Skill 的硬约束要求「执行器联网检索能力先探活,不通则如实降级」
——本文件记录真实探测的历史结果。**任何失败都是合法结果,如实记录即是完
成**;通/不通/账号配额/网关抖动都要写清楚是哪一种,不能只在"通"的时候
才落笔。

探针脚本:`scripts/probe-competitive-analysis-search.sh`(真实 `claude -p`
CLI 单轮联网检索小任务,预算用 `BW_CLAUDE_MAX_BUDGET_USD` 封顶,超时+最
多 2 次瞬时网关退避重试兜底,详见脚本头部注释)。**本探针只在监理脚本形
态下手动跑,不是常绿 CI 步骤**——和 `scripts/supervise-real-demo.sh` 同
一条纪律(CLAUDE.md「真实 claude 执行只在 example/监理脚本里跑」)。

---

## 2026-07-23 · 首次真实探测

**探测方式**:`./scripts/probe-competitive-analysis-search.sh`(手动执行,
两次真实调用,均通过本机 `claude` CLI `2.1.217`,未经 stub/mock)。探针提
示词要求检索"Rust 编程语言当前最新稳定版本号"并给出来源 URL,附带"没有
检索工具就明确声明"的诚实指令。

### 尝试 1 · `--max-budget-usd 0.15`(脚本当时的默认值)

结果:`is_error: true`,`subtype: "error_max_budget_usd"`,
`errors: ["Reached maximum budget ($0.15)"]`。

原始 JSON 关键字段:

```json
{
  "subtype": "error_max_budget_usd",
  "is_error": true,
  "num_turns": 2,
  "stop_reason": "tool_use",
  "total_cost_usd": 0.1666311,
  "terminal_reason": "budget_exhausted",
  "modelUsage": {
    "claude-haiku-4-5-20251001": { "webSearchRequests": 1, "costUSD": 0.021987 }
  },
  "errors": ["Reached maximum budget ($0.15)"]
}
```

**关键证据**:`modelUsage.claude-haiku-4-5-20251001.webSearchRequests` 为
`1`——检索子调用**确实真实发生了一次**(不是"没有检索工具"),但
`stop_reason: "tool_use"` 说明主模型还没来得及汇总检索结果、引用来源就
先撞上了 `$0.15` 的预算上限,被腰斩在半路(`terminal_reason:
"budget_exhausted"`)。

**分类**:不是网关抖动(无 529/503 类文本)、不是账号配额(无 429/quota
类文本)、也不是"无检索工具"——是**探针给的预算配置过低**,真实一次
完整调用大约需要 $0.15–$0.17,卡在 $0.15 的上限上必然失败。

### 尝试 2 · `--max-budget-usd 0.30`(据尝试 1 的证据调高后重跑)

结果:`is_error: false`,`subtype: "success"`,`stop_reason: "end_turn"`,
`total_cost_usd: 0.1534237`(实际花费低于新上限,尝试 1 失败前的花费与本
次真实花费的差异来自 prompt-cache 命中率的自然波动,不是异常)。

原始返回文本(`result` 字段,完整,未删减):

> Rust 编程语言当前最新稳定版本是 **1.97.1**(2026-07-16 发布),前一个
> 版本 1.97.0 于 2026-07-09 发布公告。Rust 保持每 6 周一个稳定版的发布节
> 奏。
>
> Sources:
> - [Announcing Rust 1.97.0 | Rust Blog](https://blog.rust-lang.org/2026/07/09/Rust-1.97.0/)
> - [Rust | endoflife.date](https://endoflife.date/rust)
> - [Rust Versions | Rust Changelogs](https://releases.rs/)

`modelUsage.claude-haiku-4-5-20251001.webSearchRequests: 1`,与尝试 1 一
致——同一条检索子调用这次跑完了全程,主模型拿到检索结果后正常汇总、正
常引用来源 URL。

### 结论

**通 —— 检索可用。**

- 本机 `claude` CLI(经用户自己的 `ANTHROPIC_BASE_URL`/鉴权配置,子进程
  按 `claude_cli.rs` 同一条纪律剥离了宿主嵌套会话的临时凭据后正常发起真
  实检索)具备真实联网检索能力,`web_search` 工具被真实调用并返回可用结
  果。
- 竞品分析 Skill 的「路径 A(检索可用)」在 2026-07-23 这次真实探测下是
  可用路径——本次探测**不构成对未来每次运行都通的保证**(账号配额/网关
  抖动是已知的真实风险,memory 记录过 2026-07-20 曾撞上 429、重置日
  2026-07-24),每次真实跑竞品分析活时仍应按 Skill 正文「检索能力探活」
  一节里的判定依据,对**当次会话**做出独立判断,不能只凭这条历史记录就
  跳过探活直接假设可用。
- **副产品修正**:探针脚本的默认预算已从 $0.15 上调到 $0.30(见脚本头
  部注释里对这次真实测试结果的引用)——$0.15 对一次"检索+汇总+引用"
  的完整单轮调用不够用,这是本次探测顺带发现、已经修正的一个配置缺
  陷,不是留着不管的已知问题。

**未触发的分支**(如实记录"没测到"是什么,不是回避):这次两次尝试都不
是网关抖动(无 529/503/502/504/"访问量过大"类错误文本)、也不是账号配
额耗尽(无 429/quota/rate-limit 类错误文本)——如果未来某次真实跑撞上这
两类,应该在本文件追加一条新的时间戳记录,而不是覆盖这条 2026-07-23 的
记录(本文件是历史记录,不是滚动覆盖的状态文件)。
