# Claude Workflow 标准实现方案

> 我们 Loop / workflow 的**底层执行引擎就是 Claude 的 Workflow 工具**(动态模式)。本文把它的执行模型、API、标准骨架与硬约束固化下来,作为本项目所有 workflow 的标准实现参考。
>
> 标注约定:**〔文档〕** = 工具契约保证的;**〔实测〕** = 我们在本会话里解剖 4 次真跑的 session 日志(`<session>/subagents/workflows/wf_*/agent-*.jsonl` + `journal.jsonl`)亲眼验证的。

---

## 0. 一句话

一个 workflow = 一段**确定性 JS 编排脚本**:用 `agent()` 派子 agent、用 `pipeline()/parallel()` 组织并发、用 `schema` 收结构化结果。**子 agent 是完整 agent**(能调你已有的 skill、能渲染自查),不是一次性补全。所以「基于已有资产跑 workflow」是真的。

---

## 1. 执行模型(玩法 · 实测)

1. **子 agent = 完整 agent,不是单次补全。**〔实测〕它会先文字推理 → 用 `Bash` 把数据写进 /tmp 自查(如归类 156 skill 时自己 `cat>` 计数、查重,发现 gsap 重复)→ 才产出。
2. **子 agent 自带你的全部家底。**〔实测〕每个 agent 的对话开头被注入两个 attachment:
   - `skill_listing`:项目全部 skill 的名称+描述(我们这是 156 个)。
   - `deferred_tools_delta`:可 `ToolSearch` 按需加载的工具(WebSearch / Claude Preview MCP / Cron 等)。
   → **因此子 agent 能用 `Skill` 工具真的调用你已有的 skill**,并 `Read` 对应 `.claude/skills/<name>/SKILL.md`。落地页那次,4 个阶段分别真调了 `reference-design-contract / copywriting / design-taste-frontend / design-review`。
3. **会渲染、会自查。**〔实测〕`design-taste-frontend` 阶段 ToolSearch 拉 **Claude Preview MCP**,`preview_start → eval/screenshot/inspect` 边看边改;`design-review` 阶段截了 11 张图做真视觉审计,才给 9.2 分。**带渲染-自查闭环**。
4. **`schema` → 强制 `StructuredOutput` 工具收口。**〔文档+实测〕传了 schema,子 agent 最后必须调用 `StructuredOutput` 提交对象,校验不过会重试;`agent()` 直接返回校验后的对象,无需解析。
5. **缓存 / resume 按内容哈希。**〔实测〕`journal.jsonl` 对每个 `agent()` 调用记 `started` / `result` 两条,key 是 `v2:<sha>`(prompt + opts 的指纹)。同 prompt 同 key → resume 时直接返缓存。
6. **并发有上限。**〔文档〕同时运行的 `agent()` ≤ `min(16, CPU核-2)`,超出排队;单次 `parallel/pipeline` ≤ 4096 项;一个 workflow 生涯总 agent ≤ 1000。

---

## 2. API 速查〔文档〕

```js
export const meta = {                 // 必须,且是纯字面量(不能有变量/函数/拼接)
  name: 'kebab-name',
  description: '一句话,权限弹窗会显示',
  phases: [{ title: '阶段A' }, { title: '阶段B' }],  // 与 phase() 标题逐字对应
}
// 函数体直接 await:
await agent(prompt, { label, phase, schema, model, isolation:'worktree', agentType })
await pipeline(items, stage1, stage2, ...)   // 默认:无屏障,每项独立穿过所有阶段
await parallel([()=>..., ()=>...])           // 屏障:等齐所有;失败项→null,记得 .filter(Boolean)
phase('阶段A'); log('给用户的进度行')
args            // Workflow 调用时传入的 JSON,原样可读
budget          // { total, spent(), remaining() } —— 按用户 "+500k" 指令做动态深度
await workflow(nameOrRef, args)              // 内联跑另一个 workflow(只能嵌一层)
```

- `agent()` 无 schema → 返回最终文本(string);有 schema → 返回校验后的对象;被跳过/终态错误 → `null`。
- `opts.phase` 在 pipeline/parallel 的 stage 里**显式指定**,避免和全局 `phase()` 抢状态。
- `opts.model` 默认**省略**(继承主循环模型,通常就对);只有高度确信某档更合适才设。
- `opts.isolation:'worktree'` 仅当多个 agent 会并行改同一批文件、否则会冲突时才用(贵)。

---

## 3. 标准骨架(照抄)

```js
export const meta = {
  name: 'my-workflow',
  description: '做什么',
  phases: [{ title: '审' }, { title: '验' }],
}
const SCHEMA = { type:'object', properties:{ ok:{type:'boolean'}, items:{type:'array',items:{type:'string'}} }, required:['ok'] }

const DIMS = [{key:'a', prompt:'...'}, {key:'b', prompt:'...'}]
const results = await pipeline(
  DIMS,
  d => agent(d.prompt, { label:`审:${d.key}`, phase:'审', schema:SCHEMA }),
  (r, d) => parallel((r.items||[]).map(it => () =>
    agent(`对抗验证:${it}`, { label:`验:${d.key}`, phase:'验', schema:VERDICT })
      .then(v => ({ it, v }))))
)
return { confirmed: results.flat().filter(Boolean) }
```

**默认用 `pipeline()`**(无屏障,墙钟=最慢单条链)。只有「阶段 N 需要 N-1 的全部结果」(去重、早退、互比)才用 `parallel()` 屏障。

---

## 4. 五个标准模式

| 模式 | 何时用 | 形态 |
|---|---|---|
| **pipeline(默认)** | 多阶段、每项独立 | `pipeline(items, s1, s2)`,阶段间无屏障 |
| **parallel(屏障)** | 下一步需全部结果(去重/早退/互比) | `await parallel(thunks)` 后再处理 |
| **loop-until-goal(执行器 Loop)** | 闭合任务,跑到验收自停 | `while(!passed && i<MAX){ 跑阶段; 验收; }`,见 `skills/create-loop/templates/loop.workflow.js` |
| **monitor(监测器)** | 开放环节,不求终止 | 单轮 + `ScheduleWakeup` 周期触发;产物=投喂新 Goal,不计入待办 |
| **对抗验证 / 评审panel** | 怕假阳性 | 对每个发现派 N 个独立怀疑者,多数否决则杀;或多视角(正确性/安全/可复现)各审一遍 |

---

## 5. 我们的 Loop 架构 ↔ Workflow 映射

> Loop = **清晰 Query + 固定 Workflow 模板 + Agent Team + 清晰 Goal**

| Loop 部件 | 映射到 Workflow |
|---|---|
| Query | 脚本里给 `agent()` 的 prompt(`args.query`) |
| 固定 Workflow 模板 | 脚本的阶段循环(`for stage of stages`),不是自由画布 |
| Agent Team | 每阶段一个 `agent()`,`stages[].{prompt, model}` |
| Goal | `acceptance` 字符串,由"验收"阶段的 agent 逐轮判定;达成则 return,自停 |
| **调用已有 skill** | 阶段 prompt 写「**运用你具备的 `<skill>` skill**」即触发真实调用(实测有效) |

---

## 6. 两道闸的实现约定(重要)

机器查不了「验收够不够充分」(领域特定)。所以:

- **两道闸不在脚本里自动放行。** 脚本只跑"可自检"的那部分验收(测试转绿、契约通过、保真零漂移)。
- **验收闸**:`acceptance` 必须是**人已认证**的达标线(工作台在调用脚本前确保;命中已认证模式才由飞轮放行)。
- **副作用闸**:`sideEffects` 列出的对外/花钱/不可逆动作,由工作台在脚本**外围**拦截、人批后才真正发生。脚本里凡用到付费 skill(`fal-* / venice-* / imagegen / sora / speech / replicate` 等)必须在 `sideEffects` 标出。

---

## 7. 硬约束与坑〔文档〕

- **不能用** `Date.now()` / `Math.random()` / 无参 `new Date()`(会破坏 resume)。要时间戳就 `args` 传入或返回后再盖;要随机就按 index 变 prompt/label。
- `meta` 必须纯字面量(no 变量/拼接/spread)。
- 脚本是 **JS 不是 TS**(类型注解 `: string[]`、interface、泛型都会解析失败)。
- 没有文件系统 / Node API(但**子 agent**有 Read/Write/Bash/MCP)。
- `parallel` 的 thunk 抛错 → 该项 `null`(整体不 reject);用前 `.filter(Boolean)`。
- pipeline 某阶段抛错 → 该项掉成 `null`,跳过其余阶段。

---

## 8. resume / 迭代〔文档〕

每次 `Workflow` 调用都会把脚本存到 `<session>/workflows/scripts/<name>-<runId>.js` 并在结果里回传路径。迭代:用 `Write/Edit` 改那个文件,再 `Workflow({scriptPath, resumeFromRunId})` —— **未改动的 agent() 前缀直接返缓存**,第一个改动处及之后才重跑。同脚本同 args → 100% 命中。

---

## 9. 实测证据(本会话的 4 次真跑)

| 运行 | 形态 | 验证了什么 |
|---|---|---|
| 对抗式多方案设计 | pipeline(设计→对抗) + 收敛 | 7 agent;judge panel + 收敛 |
| 盘点 156 skill → 找可组合 Loop | 3 段链 + schema | 子 agent Bash 自查计数/查重;StructuredOutput;校验零编造 |
| 落地页 Loop(实跑) | loop-until-goal | 子 agent **真调 4 个 skill** + Preview MCP 渲染截图;9.2 过闸第 1 轮自停 |
| 用 skill 审 UX 求简洁 | parallel(4 视角) + 收敛 | design-review 真渲染 11 截图;多视角收敛出 16 条简化 |

---

## 10. 速查:何时不要用 workflow

- 单步、对话式、或纯机械小改 → 直接做,别起 workflow(贵)。
- 需要人在环做不可自检的判断 → 那是"闸",放工作台外围,不塞进脚本。
- 只是想并发读几个文件 → 主循环并行工具调用即可。
