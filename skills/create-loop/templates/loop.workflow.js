// ─────────────────────────────────────────────────────────────
// Loop 固定 Workflow 模板 · 对齐 Claude Workflow 工具(动态模式)
//
// Loop = Query + 固定 Workflow 模板(本文件) + Agent Team + Goal
//
// 用法:把一份 Loop 规格作为 Workflow 工具的 `args` 传入。规格形如:
//   {
//     query:      "一句话目标翻成的清晰 prompt",
//     goal:       "一句话 Goal",
//     acceptance: "人已认证的、可观测、抗漂移的达成标准(多行)",
//     stages: [   // 固定 workflow 的阶段 + Agent Team(角色+模型档)
//       { name:"受理", prompt:"...", model:"haiku" },
//       { name:"复现", prompt:"...", model:"sonnet" },
//       { name:"修复", prompt:"...", model:"opus" },
//       { name:"评审", prompt:"...", model:"opus" }
//     ],
//     kind:        "executor",        // executor=跑到验收自停;monitor 见文末说明
//     maxIters:    3,
//     sideEffects: ["合并到主干"]      // 运行时由副作用闸在脚本外围把关
//   }
//
// 两道闸不在脚本内自动通过:
//   · 验收闸 —— args.acceptance 必须是「人已认证」的标准(工作台在调用前确保)
//   · 副作用闸 —— args.sideEffects 列出的动作由工作台拦截、人批后才真正发生
// ─────────────────────────────────────────────────────────────

export const meta = {
  name: 'loop-runner',
  description: '通用 Loop 执行器:Agent Team 跑固定阶段 → 对照已认证 Goal 验收 → 未达成则带缺口迭代,达成自停',
  phases: [
    { title: '执行', detail: 'Agent Team 按固定阶段接力推进' },
    { title: '验收', detail: '对照 Goal 验收标准判定;未过则回到执行' },
  ],
}

const LOOP = args || {}
const MAX = LOOP.maxIters || 3

const VERDICT = {
  type: 'object',
  properties: {
    passed: { type: 'boolean', description: '是否真的达成 Goal 验收标准' },
    reason: { type: 'string' },
    gaps:   { type: 'array', items: { type: 'string' }, description: '未达成时,具体差在哪(喂给下一轮)' },
  },
  required: ['passed', 'reason'],
}

if (!LOOP.query || !LOOP.acceptance || !Array.isArray(LOOP.stages)) {
  log('Loop 规格不完整:需要 query / acceptance / stages。')
  return { error: 'invalid loop spec', got: Object.keys(LOOP) }
}

let iter = 0, passed = false, output = null
const history = []

while (!passed && iter < MAX) {
  iter++

  // ── 执行:Agent Team 按固定 workflow 阶段接力 ──
  phase('执行')
  let ctx = `【Query · 目标】\n${LOOP.query}`
  if (iter > 1) {
    const lastGaps = history[history.length - 1].gaps
    ctx += `\n\n【上一轮验收未过,本轮必须补齐这些缺口】\n- ${(lastGaps || ['(未给具体缺口)']).join('\n- ')}`
  }
  for (const st of LOOP.stages) {
    ctx = await agent(
      `你是 Loop 里的「${st.name}」环节。\n${st.prompt}\n\n【上一步交接】\n${ctx}\n\n只产出你这一环的结果,供下一环接力。`,
      { label: `${st.name} · 第${iter}轮`, phase: '执行', model: st.model }
    )
  }
  output = ctx

  // ── 验收:对照人已认证的 Goal 标准(验收闸的可自检部分)──
  phase('验收')
  const v = await agent(
    `严格对照下面这条【人已认证的 Goal 验收标准】,判断本轮产出是否真的达成。\n` +
    `默认怀疑:只要不充分、可能漂移、或无法客观验证,就判 passed=false 并写清缺口。\n\n` +
    `【验收标准】\n${LOOP.acceptance}\n\n【本轮产出】\n${output}`,
    { label: `验收 · 第${iter}轮`, phase: '验收', schema: VERDICT }
  )
  history.push({ iter, passed: v.passed, reason: v.reason, gaps: v.gaps || [] })
  passed = v.passed
  log(`第 ${iter}/${MAX} 轮 — ${passed ? '✓ 达成 Goal,自停' : '未达成:' + v.reason}`)
}

return {
  loop: LOOP.query,
  kind: LOOP.kind || 'executor',
  passed,
  iters: iter,
  output,
  history,
  // 收口:达成则交付;否则浮出给人(转人工 / 改 Goal / 改 Agent Team),不假装成功
  next: passed
    ? (LOOP.sideEffects && LOOP.sideEffects.length
        ? `达成 Goal。但含副作用 [${LOOP.sideEffects.join(', ')}] → 交副作用闸人批后才真正发生。`
        : '达成 Goal,无副作用,可直接收口。')
    : `达 ${MAX} 轮上限仍未达成 → 浮出给 Builder:转人工 / 收紧 Goal / 换 Agent Team。`,
}

// ── 监测器(开放环节)怎么用本模板 ──
// 监测器不求终止:把上面 while 改成「单轮执行」,不跑验收循环;
// 它的产物是「投喂新 Goal」——为命中条件的发现,生成一条新的执行器 Loop 规格;
// 然后用 ScheduleWakeup(动态步进)周期性再触发本脚本。
// 即:监测器 = 单轮 loop.workflow + ScheduleWakeup,不进待办计数,只在「投喂/浮出」时出声。
