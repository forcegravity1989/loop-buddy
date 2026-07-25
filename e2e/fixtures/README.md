# e2e/fixtures/

E2E 验收动作流(plan/15)用的种子库,不是随手攒的假数据——`demo.db` 是
`cargo run -p bw-app --example real_demo -- ... --mock` 真跑出来的产物,
`--mock` 只是让管线本身可以廉价、无网关依赖地被验证(CLAUDE.md 纪律 3:
E2E 绝不依赖网关),不是伪造数据。

## 来源与再生成

```bash
mkdir -p e2e/fixtures
cargo run -p bw-app --example real_demo -- e2e/fixtures/demo.db "$(mktemp -d)" --mock
sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM project; SELECT status, count(*) FROM issue GROUP BY status;"
```

`real_demo` 按会话标题幂等(重跑只补没发生过的阶段,不覆盖已发生的历史),
所以上面这条命令可以对着已存在的 `demo.db`安全重跑来刷新/补全数据。

## 真实读回(2026-07-25,首次生成)

```
$ sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM project;"
2
$ sqlite3 e2e/fixtures/demo.db "SELECT status, count(*) FROM issue GROUP BY status;"
(0 行——issue 表为空)
$ sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM issue;"
0
$ sqlite3 e2e/fixtures/demo.db "SELECT id, name, active_stage FROM project;"
b71e68ad-5496-4043-bc17-aeeaa6877f94|linkcheck-md|prototype
a01c830b-908c-4038-a55b-e1212eceb227|standup-digest|prototype
```

两个项目(`linkcheck-md`、`standup-digest`)各走完一整圈五阶段环(原型→构建→
优化→运营推广→运维→回到原型),真实交接/会话/run/信号/产物记账都在库里,
可用 `BW_DB=e2e/fixtures/demo.db cargo run -p app-desktop` 打开核验。

## 已知缺口(如实记录,不假装齐全)

`real_demo` 这个指挥器驱动的是**五阶段工作流环**(`op_stage`/`workflow_run`/
`session` 表),从不创建 **Issue 卡**(`issue` 表)——通篇代码没有一处涉及
`Command::CreateIssue`/`RunIssue`/`TransitionIssue`。因此这份 fixture 里
`issue` 表是空的:**没有可开工的 Issue,也没有 Done 的 Issue。**

plan/15 §4 的常青流「② 跑 Issue(Mock)」「③ 人点 Done」「④ 蒸馏成技能」都
需要 fixture 里已有(runnable 的/Done 的)Issue 卡才能跑——这份 fixture 目前
**不满足**它们的前置条件。这是 `real_demo --mock` 生成器本身覆盖范围的缺口,
不是本票允许手改数据库来填的东西(CLAUDE.md 纪律 1:数字只读回,绝不硬编;
fixture 必须是真跑的产物,不能手工塞行)。留给下一票/controller 决定怎么补
(例如另起一个会创建+跑+完成 Issue 的生成步骤,或扩展 `real_demo` 本身)。

## 只跑副本的纪律

任何验收流**绝不**直接打开这份 `demo.db`(更不会打开用户真实的日常库)。
统一经 `scripts/flow-prep.sh fixture:demo <run-dir>` 把它 `cp` 到一个带时间戳
的临时路径,再用 `BW_DB=<临时路径>` 注入给被测应用。原 fixture 与真实日常库
都只被 `cp` 读,绝不被打开、绝不被写。
