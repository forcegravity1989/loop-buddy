# v3 → v4 升级计划（五阶段五方法论落地到向导）

状态：`Builders工作台-项目管理向导 v4.dc.html` 已从 v3 原样复制，**尚未做任何修改**。
⚠ run_script 的 readFile/saveFile 不接受中文文件名（已验证报 disallowed characters）——
所有编辑必须用 `dc_js_str_replace` / `dc_html_str_replace` 逐条做（b_multi 用于全局更名）。
设计依据：《体系重构方案 v2 · 五阶段五方法论.dc.html》（已交付）。

## 核心设计决策
- 五段主轴：原型(#C5654A 原型师/假设驱动探索) → 构建(#CC8B3C 构建师/规格驱动交付) → 优化(#6E8C5A 优化师/度量驱动打磨) → 运营推广(#4F7E86 运营推广师/增长实验) → 运维(#8A8275 运维师/可靠性工程SRE)。
- 向导步骤映射 steps：原型[1,2,6]、构建[]、优化[]、运营推广[]、运维[7]；指标步骤 03–05 不占主轴（属顶部指标层胶囊，处于这些步骤时主轴无高亮=教学点）。
- 环节映射 envs（泳道/APP阶段轴用）：原型[1,2,3,6]、构建[4]、优化[]、运营推广[]、运维[5,7]；stageRole 改 3→原型师、6→原型师（4 构建师、5/7 运维师不变）。
- 无步骤的三段点击进「阶段舱」（cap: build/opt/grow）：换装卡（默认视图+引领焦点+AI编队）、方法循环、样板项目状态、DoD 交棒清单（可勾选，未勾满=带险交棒）、反模式条、上一段/交棒下一段按钮。step6 confirm → cap 'build'，链式 build→opt→grow→go(7)。
- step6 加「原型段交棒清单」（dod.proto=[true,true,false]）+按钮改「交棒给构建师 · 进入构建段 →」；step7 加「复盘回流·线闭成环」卡（onReflux → ✓已回流01洞察板 + goStep1 链接）。
- 全局更名：构建者→构建师、清理者→优化师、增长者→运营推广师、维护者→运维师；Sweeper→Optimizer；周期第三程 运维→成熟（state.projects p1 cycle 同改）。
- cycleList：phases 改「主环 · …」、need 改角色名、mix 改驻留分布 探索40/30/15/10/5、扩张10/25/20/30/15、成熟5/10/25/25/35；wzMix abbr 用映射（推/维区分）。
- 创建流：标准项目管理 v2→v3、「四阶段·关口制」→「五阶段一环·交棒制」、cfMixSegs 40/30/15/10/5、「原32 构28 清20 增12 维8」→「原40 构30 优15 推10 维5」、cfStageChips 5 个（01原型hot其余cold）、「阶段 01 分析已激活」→「01 原型已激活」、cfAssetLine 方法论 v3。
- 关口→交棒：gateNames 4 个（原型→构建/构建→优化/优化→推广/推广→运维），文案「已交棒/待交棒」。
- 顶栏：「方法论层 · 贯穿四阶段」→「指标层 · 贯穿五阶段」；「流程主轴」→「流程主轴 · 五段一环」；节点加角色小字(roleName/roleColor)+sub=方法论名，min-width 128，count 5，行尾加 ↩（title=运维复盘回流原型）。
- step0：五角色卡 Sweeper→Optimizer；三条周期 bar 宽度改驻留分布+右侧文案改主环；「运维/Sustain」行→「成熟」；横纵网 4 黑盒→5 黑盒(名+方法论小字+role色顶边)、drops 5 列（洞察密度·假设圈速/假设命中率｜CI通过率·评审周转/交付周期·缺陷密度｜预算达标率·债务燃尽/P95·单位成本｜周实验数·激活率/留存·获客成本｜错误预算余量·MTTR/可用性·事故数）；开场步骤串加「构建 · 优化 · 运营推广（三个阶段舱）」；按钮旁「约 7 步」→「7 步 + 3 个阶段舱」。
- 泳道：两处 `158px repeat(4,1fr)`→repeat(5,1fr)；swimPhaseHeaders/ phaseChips hint 4→5；「横向 4 阶段」→「横向 5 段主轴」；cells 过滤用 (b.envs||b.steps)。

## 逻辑层编辑清单（dc_js_str_replace，m0057 那次失败脚本里有全部精确 find/replace 文本，照搬即可）
1. 全局更名 ×4（b_multi:true）
2. state 插入 cap/dod/reflux（在 completed:{} 后）；p1 cycle 成熟
3. go() 加 cap:null；confirm() 加 step6 分支
4. next/prev 后插 openCap/capNext/capPrev/toggleDod/capDefs/capVals（完整代码在 m0057 脚本）
5. phaseBuckets 替换为五段（含 envs/cap/role/color）
6. cycleList 替换（注意：更名后 mix 里已是"师"名）
7. stageRole 3、6 两行替换
8. swimGrid items 过滤 envs；phaseChips 块（bEnvs/gateNames/onClick 判空/gateTitle）
9. wizPhases 构建器替换（capKey + 角色色高亮 + openCap）
10. isStep0-7 加 && !capKey + ...this.capVals()
11. wzMix abbr 映射；cfMixSegs/cfStageChips/cfAssetLine
12. lensDef swimlane desc；泳道注释「四阶段流程轴」→「五段主轴」

## 模板层编辑清单（dc_html_str_replace）
B1 指标层文案 B2 主轴label B3 主轴节点块整体替换（+↩） B4 Sweeper→Optimizer
B5 step0 三条 bar+右侧文案+成熟label B6 step0 横纵网两个 grid+前导段落 B7 周期段落（探索/扩张/成熟）
B8 wzAgents 角色名 width:48px→70px B9 开始按钮旁文案 B10 step6 eyebrow+DoD卡+按钮改交棒
B11 step7 eyebrow+复盘回流卡（插在 nav 前，nav 用「完成，生成项目看板」定位）
B12 阶段舱大块插入（定位：step7 的 `</sc-if>` 与 isWizard 收尾 `</sc-if>` 之间、PROJECTS HOME 注释前）——完整 markup 见 m0057 计划（capIdx/capName/capRole/capLoop/capSample/换装暗卡/capDod 清单/反模式条/onCapPrev/onCapNext）
B13 创建流 4 处文案 B14 泳道 repeat(5,1fr)×2 + 文案 + hint×2 B15 phaseChips hint 4→5 + STAGE AXIS 注释
B16 step0 开场步骤串加阶段舱提及

完成后：ready_for_verification('Builders工作台-项目管理向导 v4.dc.html')；重点自查：主轴 5 节点高亮随 step/cap 切换、step6 交棒→三舱链→step7、复盘回流按钮、泳道 5 列、创建流草案 5 阶段 chips、周期「成熟」贯通（projects home chip 不报 undefined）。
