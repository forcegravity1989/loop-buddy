# verification/ — Builders 工作台真实验证产物

存放"事情真的发生过"的证据：自包含 HTML 报告 + 演示动图，均由真实运行产出，不是设计稿。

| 文件 | 内容 |
|---|---|
| [`Builders-Workbench-Complete-Form-Report.html`](Builders-Workbench-Complete-Form-Report.html) | 完整形态演示报告（初版），嵌入 [`../docs/board-issues.png`](../docs/board-issues.png) |
| [`BW-Complete-Form-Report.html`](BW-Complete-Form-Report.html) | 完整形态演示报告（multica × BW 融合后一版），自包含无外部引用 |
| [`WorkflowHub-25-Iterations-Report.html`](WorkflowHub-25-Iterations-Report.html) | WorkflowHub 25 轮五角色五阶段自举报告（capstone），内嵌下方两个 APNG |
| `WorkflowHub-Demo.apng` / `WorkflowHub-Demo-embed.apng` | 25 轮自驱优化演示动图（完整版 1280×720 / 嵌入版），由 `scripts/make_demo_video.py` 逐帧渲染 |

## 和 design/ 的区别

`design/` 是交互原型稿——待评审、待实现，会持续修改。这里的东西是**跑出来的结果记录**：改代码不会让旧报告变化，它们是某一时刻真实运行的快照。

## 未纳入本目录的相关材料

`docs/*.png`（部分被 `iterations/*.md` 交叉引用）和 `iterations/`（过程日志、交接报告）里也有验证性质的截图和记录，但它们互相引用较深、搬动代价高于收益，本次整理未动。
