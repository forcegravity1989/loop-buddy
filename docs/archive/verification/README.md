# verification/ — Builders 工作台真实验证产物

> ⚠️ **历史档案(2026-07-15 冻结,2026-08-17 从仓库根 `verification/` 归档到此;`docs/*.png` 四张历史截图一并搬入)**。这里是 2026-07 中旬「完整形态」与「WorkflowHub 25 轮自举」两批的演示报告与动图,证明当时的事情真发生过,不描述现状。现在的验证手段见 `CLAUDE.md`「核心纪律」(深链启动 + sqlite 读回 + `pty_smoke` 等 headless 例子)。两份「完整形态演示报告」内容近似,**以 `BW-Complete-Form-Report.html`(融合后一版)为准**。

存放"事情真的发生过"的证据：自包含 HTML 报告 + 演示动图，均由真实运行产出，不是设计稿。

| 文件 | 内容 |
|---|---|
| [`Builders-Workbench-Complete-Form-Report.html`](Builders-Workbench-Complete-Form-Report.html) | 完整形态演示报告（初版），嵌入 [`board-issues.png`](board-issues.png) |
| [`BW-Complete-Form-Report.html`](BW-Complete-Form-Report.html) | 完整形态演示报告（multica × BW 融合后一版），自包含无外部引用 |
| [`WorkflowHub-25-Iterations-Report.html`](WorkflowHub-25-Iterations-Report.html) | WorkflowHub 25 轮五角色五阶段自举报告（capstone），内嵌下方两个 APNG |
| `WorkflowHub-Demo.apng` / `WorkflowHub-Demo-embed.apng` | 25 轮自驱优化演示动图（完整版 1280×720 / 嵌入版），由 `../scripts/make_demo_video.py`(原 `scripts/make_demo_video.py`,随本目录一并归档) 逐帧渲染 |

## 和 design/ 的区别

`design/` 是交互原型稿——待评审、待实现，会持续修改。这里的东西是**跑出来的结果记录**：改代码不会让旧报告变化，它们是某一时刻真实运行的快照。

## 未纳入本目录的相关材料

`docs/*.png`（部分被 `iterations/*.md` 交叉引用）和 `iterations/`（过程日志、交接报告）里也有验证性质的截图和记录，但它们互相引用较深、搬动代价高于收益，本次整理未动。
