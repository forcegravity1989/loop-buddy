# Cron 标准(BW 组件规范 · cron_task)

`cron_task` 是这个项目的**例行节奏**。BW 的铁律是「定时任务只自动建活,绝不自动完活」——\
这份文件先讲字段,再讲这条铁律具体怎么落在字段上。

## 字段:作者填 vs 系统派生

| 字段 | 谁填 | 说明 |
|---|---|---|
| `name` | 作者 | 人看的任务名。 |
| `target` | 作者 | 自由文本——到点要跑的东西(通常是某条 workflow 的名字);不是硬外键,\
因为目标可能是 hub workflow、也可能是一次 connector 同步。 |
| `schedule` | 作者 | `Cadence`(如 `weekly` / `daily` / `real_time`)。 |
| `project_id` | 作者 | `None` = 全部项目;通常新建时填当前项目。 |
| `mode` | 作者 | **只有两个合法值,别的都是编造**:`run_workflow`(到点跑一条 workflow,\
默认)、`create_issue`(到点只建一张 Issue,不跑任何东西——autopilot 的 no-hijack \
设计)。 |
| `issue_stage` | 作者(仅 `mode=create_issue` 时有意义) | 新建 Issue 挂哪个阶段。 |
| `issue_assignee` | 作者(仅 `mode=create_issue` 时,可选) | 按 agent **名字**指派\
(到点解析,找不到就诚实建一张未指派的 Issue,不是失败)。 |
| `status` | 半系统 | `running` / `normal` / `failed` / `paused`——`paused` 是人工\
介入的唯一手柄;其余三态由真实调度结果驱动,不是随手改的展示字段。 |
| `last_run` / `last_run_at` | **系统派生** | 真实上次触发时间(`last_run` 是显示串,\
`last_run_at` 是拿来跟"到期没到期"比较的真实时钟)。新建都留空/0。 |
| `next_run` | **系统派生/展示** | 由 `schedule` + `last_run_at` 算出的下次预期时间,\
不是手填的承诺。 |

## 「no-hijack」到底是什么意思(字段层面)

`mode=create_issue` 的任务,到点**只会**执行一次 `CreateIssue`(状态永远是 \
`Backlog`/`Todo` 起点)——它没有能力把 Issue 一路推到 `Done`,那条路径在代码里\
根本不存在。如果你想让"到点自动生成一份产出"(比如 aihot 的每日日报),正确设计是:\
cron 到点建一张「生成今日日报」的 Issue,由人(或人配置的自动指派 agent)在看板上\
走 `RunIssue` 真实执行,完成后仍然是人点 `TransitionIssue → Done`。**不存在\
"cron 直接把活标记完成"这条路**——这不是当前实现的疏漏,是故意不做。

## 创建前自查清单

1. `mode` 是不是这两个合法值之一,没有杜撰第三个?
2. 如果 `mode=create_issue`:有没有误期待它会"自动跑完"这件事——它只负责\
"到点提醒有活要干",不负责干活?
3. `last_run` / `last_run_at` / `next_run` 是否留给系统,没有手填一个假的\
"看起来已经跑过"的时间戳?
