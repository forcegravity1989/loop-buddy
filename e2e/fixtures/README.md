# e2e/fixtures/

E2E 验收动作流(plan/15)用的种子库,不是随手攒的假数据——`demo.db` 是两步
真实生成的产物:

1. `real_demo --mock` 走完两个项目的完整五阶段环(`--mock` 只是让管线本身
   可以廉价、无网关依赖地被验证——CLAUDE.md 纪律 3:E2E 绝不依赖网关,不是
   伪造数据);
2. `seed_fixture` 打开同一个库,对 `linkcheck-md` 项目补三张 Issue 卡——
   全部经真实 `Command`(`CreateIssue`/`TransitionIssue`)派生,状态机合法
   转移表、settle-once 记账、`settled_at` 打点全部真实触发,**没有一行是
   裸 SQL 塞进去的**。

## 来源与再生成(两步,顺序不能反)

```bash
mkdir -p e2e/fixtures
# 第一步:五阶段环(两个项目)
cargo run -p bw-app --example real_demo -- e2e/fixtures/demo.db "$(mktemp -d)" --mock
# 第二步:补三张 Issue(Todo/InReview/Done 各一),真 Command 播种
cargo run -p bw-app --example seed_fixture -- e2e/fixtures/demo.db
# 读回核验
sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM project; SELECT status, count(*) FROM issue GROUP BY status;"
```

两步都是幂等的:`real_demo` 按会话标题幂等(重跑只补没发生过的阶段,不覆盖
已发生的历史);`seed_fixture` 按 Issue 标题幂等(已存在的三张 fixture Issue
只补到目标状态缺的那几步,不重复创建、不越过目标状态)。所以上面两条命令都
可以对着已存在的 `demo.db` 安全重跑。

## 真实读回(2026-07-25,补齐 Issue 后)

```
$ sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM project;"
2
$ sqlite3 e2e/fixtures/demo.db "SELECT id, name, active_stage, workspace_path FROM project;"
101e28cf-7600-4d69-a9b3-91f0ae5548d5|linkcheck-md|prototype|
b1ee6eb4-c129-4d30-9658-96f74c2efeb2|standup-digest|prototype|
$ sqlite3 e2e/fixtures/demo.db "SELECT count(*) FROM issue;"
3
$ sqlite3 e2e/fixtures/demo.db "SELECT status, count(*) FROM issue GROUP BY status;"
done|1
in_review|1
todo|1
$ sqlite3 e2e/fixtures/demo.db "SELECT number, title, status, stage, priority, settled_at FROM issue ORDER BY number;"
1|【fixture】支持 --ignore-pattern 排除白名单路径的死链检查|todo|build|medium|
2|【fixture】修复相对路径链接在反斜杠路径分隔符下的误报|in_review|build|high|
3|【fixture】为死链检查器加 --version 参数|done|build|low|1784942047
$ sqlite3 e2e/fixtures/demo.db "SELECT number, title, settled_at, (settled_at IS NOT NULL) AS settled_ok FROM issue WHERE status='done';"
3|【fixture】为死链检查器加 --version 参数|1784942047|1
```

两个项目(`linkcheck-md`、`standup-digest`)各走完一整圈五阶段环(原型→构建→
优化→运营推广→运维→回到原型),真实交接/会话/run/信号/产物记账都在库里。
`linkcheck-md` 额外挂了三张 Issue 卡,标题都带「【fixture】」前缀(不会被误
当成真实待办),`workspace_path` 全程为空——两个项目、三张 Issue 卡的每一次
`RunIssue`/剧本执行都留在 `MockExecutor` 上,不碰真实网关。可用
`BW_DB=e2e/fixtures/demo.db cargo run -p app-desktop` 打开核验。

三张卡分别覆盖 plan/15 §4 的三个常青验收点:
- **Todo**「【fixture】支持 --ignore-pattern…」→「▶ 跑」流的可开工卡;
- **InReview**「【fixture】修复相对路径链接…」→「✓ 确认完成(人裁)」流,同时
  验证「Done 的入边只有 InReview」(这张卡被真实走到 InReview 就停,不会被
  本例程自动推进到 Done);
- **Done**「【fixture】为死链检查器加 --version 参数」→「⚗ 蒸馏为技能」流,
  真实走完 Backlog→Todo→InProgress→InReview→Done 五格转移,`settled_at`
  非空(见上面读回:`1784942047`),证明 settle-once 记账真实触发过。

## 已知缺口(如实记录)

无。上一版记录的缺口(`real_demo` 从不创建 Issue,fixture 里 `issue` 表为
空)已由 `seed_fixture` 关闭——见上面的真实读回。

## 只跑副本的纪律

任何验收流**绝不**直接打开这份 `demo.db`(更不会打开用户真实的日常库)。
统一经 `scripts/flow-prep.sh fixture:demo <run-dir>` 把它 `cp` 到一个带时间戳
的临时路径,再用 `BW_DB=<临时路径>` 注入给被测应用。原 fixture 与真实日常库
都只被 `cp` 读,绝不被打开、绝不被写。
