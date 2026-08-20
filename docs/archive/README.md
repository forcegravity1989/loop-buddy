# docs/archive/ — 历史档案(只加不改)

> **30 秒导读**:这里放**已经执行完毕、或已被后续文档取代**的计划、交接记录、设计稿与验证产物。它们记录「当时为什么这么做」,不描述现状。给两类人看:考古某个决定来龙去脉的人,和被源码/文档里的 `plan/NN §M` 锚点指到这里的人。**现状一律以仓库根的 `CLAUDE.md`、`plan/`(现役 7 篇)、`docs/v1~v3-prototype/` 为准**。2026-08-17 建目录(减负重构会话,见 `docs/superpowers/specs/2026-08-17-debt-reduction-refactor-design.md` §3)。

## 规则

1. **只加不改**:归档件搬进来后不再改正文;顶部横幅是唯一允许的追加(注明「历史档案」+「现状以 XX 为准」)。
2. **编号语义保留**:`plan/NN` 搬到 `docs/archive/plan/NN-…` 后编号不变。源码注释里的 `plan/09 §2` 这类锚点不逐条改,按号来这里找即可。
3. **相对链接可能失效**:归档件之间互相引用时写的多是搬迁前的相对路径;同一批一起搬的(plan ↔ iterations、design ↔ verification ↔ scripts)相对关系保持了,跨到现役目录的(如 `../plan/06`)会断——断了就按文件名在仓库里搜。
4. **不删**:git 历史能找回一切,但读者不该被迫翻 git log;所以搬不删。真要删(比如二进制大件),先在这里登记再删。

## 目录

| 子目录 | 从哪来 | 是什么 |
|---|---|---|
| `plan/` | 仓库根 `plan/00-05, 09-12, 14, 17-19, 21` | 早期路线选型(00-05)与做完即历史的执行批次(09-21 中的历史件)。各文件顶部有横幅;哪些段落仍有效见 [`../../plan/README.md`](../../plan/README.md) 的「已归档」表 |
| `iterations/` | 仓库根 `iterations/`(除 `PRACTICE-buddy.md`) | 交接记录(`HANDOFF-*`)、接棒报告、2026-07-15 的「V2 设计综合」(**与 `docs/v2-prototype/` 无关,只是同名**)、aihot 践行日志与数字证据、验收闸门实测证据 |
| `superpowers/` | `docs/superpowers/plans/*` 与已被取代的 `specs/` | superpowers 技能产出的实施计划原件;结论已并入 `plan/13`/`15`/`16`/`20` 或 `docs/v1-prototype/`。仍作数的 spec 留在 `docs/superpowers/specs/` |
| `design/` | 仓库根 `design/` | Rust 重写前的 HTML 交互原型稿(`.dc.html` + `support.js`)与设计评审截图,2026-07-15 冻结。用浏览器打开子目录里的 `.dc.html` 仍可渲染 |
| `verification/` | 仓库根 `verification/` + `docs/*.png` | 2026-07 中旬「完整形态」与「WorkflowHub 25 轮自举」两批的演示报告、动图(约 14MB)与截图 |
| `v4-prototype/` | `docs/v4-prototype/` 的规划期件 | V4 设计期的一次性材料:用户口述原文、二次握手问答、交叉复核记录、内部专家评审一页纸,以及结论已进设计与代码的五篇源码级预研。现状以 `docs/v4-prototype/`(现役五篇 + design/)与 `crates/bw-v4`、`crates/app-shell` 的源码为准 |
| `v1-prototype/` `v2-prototype/` `v3-prototype/` | 同名现役目录(`docs/v1~v3-prototype/`) | 2026-08-20 归档:V1/V2/V3 三代迭代线里已落地、且没有被任何现役文档引用为"当前权威"的设计事实源(共 8 篇,如 `issue1-onboard-simplify.md`、`piercing-fixes-1.md`、`roadmap.md`、`onboard-list-and-claude-resolve.md` 等)。仍被现役文档引用为当前权威、或含未关闭欠账线索的篇章原地留在同名现役目录,没有整批搬空——按篇判断,不按目录判断。现状以对应现役目录的 README 为准 |
| `scripts/` | `scripts/make_demo_video.py` | 生成上面那批演示动图的脚本;产物已归档,脚本随之归档 |
