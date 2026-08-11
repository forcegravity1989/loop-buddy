# 切片七E · 真实项目导入落库 + 六段点亮验收记录

> **30 秒导读**:这是切片七设计稿(`docs/design-next/design-s7-real-project.md`)§8.2 提交 E 的验收留证——用户在确认门已明确点头(原话「确认导入,不然我验收什么呢?」),本记录是那次点头之后的真实执行结果:一次性把用户真实日常库的三个项目(aihot 日报 / 个人画像 / WorkflowHub)导进新壳默认库,并对第一个真实项目(aihot 日报)跑了六段点亮的验收核对。**现状以本记录 + `plan/23` 进度实况表为准**;设计稿本身仍是事实源,不用来查这次真实导入的结果。全程只对旧库原件做只读操作(两次核对哈希/大小/修改时刻,逐字节相同),真写只发生在新壳默认库。第④段(真会话)按硬约束**没有**起任何真实会话,如实记为空,原因是台账第 6 条的信任对话框问题还留给用户拍板,不在本次范围。

## STATUS:GREEN(导入完成,新库是唯一写入目标;六段五亮一空,空的一段是如实的、不是漏做)

---

## 1. 旧库原件重核(只看档,零写入)

**旧库路径**:`/Users/gravity/Library/Application Support/BuildersWorkbench/workbench.db`(仓根旧壳的默认库,与切片七-3 侦察报告读到的同一份文件)。

| 检查点 | 只看档运行前 | 只看档运行后 | 真写档运行后 |
|---|---|---|---|
| 文件大小 | 2007040 字节 | 2007040 字节 | 2007040 字节 |
| 修改时刻 | 2026-08-07 17:10:45 | 2026-08-07 17:10:45 | 2026-08-07 17:10:45 |
| sha256 | `f76fb500badf2bd80f5dfd823872662a59f0ab5f7e3bd3f7958bbddc34132fe6` | 同左 | 同左 |

三次读数逐字节相同——旧库全程只读,这条比"代码里写了 `read_only(true)`"更硬的证明成立。

**命令**:

```bash
cd next && cargo run -p bw-app --example import_legacy -- \
  --from "/Users/gravity/Library/Application Support/BuildersWorkbench/workbench.db" \
  --to   "/Users/gravity/Library/Application Support/BuildersWorkbenchNext/workbench.db"
# 不给 --confirm:只看档
```

**只看档打印的计数表**,与切片七-3 侦察报告(读的是副本)逐项相同,证明**用户真实日常库这三周没有变化**,可以直接在原件上继续走真写档:

| 实体 | 条数 | 分布 |
|---|---|---|
| 项目 | 3 | WorkflowHub / aihot 日报 / 个人画像 |
| 目标(北极星) | 3 | 每项目一条 |
| 指标 | 31 | 15 条撞名(「阶段完成 Issue 数」族,逐条改名接阶段后缀)+ 16 条不撞名 |
| 观测 | 64 | aihot 60 / WorkflowHub 3 / 个人画像 1 |
| 活 | 37 | aihot 34 / 个人画像 3 / WorkflowHub 0 |
| 产物证据 | 764 | aihot 717 / 个人画像 47 / WorkflowHub 0(导成历史存档表,主控裁决 1) |
| 交棒 | 2 | 全在 WorkflowHub,1 条带险 |

拒绝清单:空。北极星来源判定:aihot 日报、个人画像判「来自正本」(正本文件北极星名字与旧库逐字相同);WorkflowHub 判「手建」(仓根正本装的是 WorkflowHub 自己的指标,名字与旧库这一行的北极星文本对不上——设计稿已点名的名实问题,本次不处置,如实沿用「手建」判定)。**判定依据原样打印进导入报告,不是导入器自己另算的摘要**,例如:

```
aihot 日报:判「来自正本」——正本 …/aihot-b7971eca/.bw/metrics.toml 读到北极星名字「每周「读完且有收获」天数」,
与旧库项目表北极星名字「每周「读完且有收获」天数」逐字相同,判「来自正本」
```

末行标记:`IMPORT_DRYRUN_OK`。

---

## 2. 真写:导进新壳默认库

**新库路径**(按 `next/crates/app-desktop/src/kernel.rs::db_path()` 的默认落点,`BW_DB` 未设时):
`/Users/gravity/Library/Application Support/BuildersWorkbenchNext/workbench.db`——运行前这个目录不存在,是一次干净的从零写入,不存在"旧数据被覆盖"的可能。

```bash
cd next && cargo run -p bw-app --example import_legacy -- \
  --from "/Users/gravity/Library/Application Support/BuildersWorkbench/workbench.db" \
  --to   "/Users/gravity/Library/Application Support/BuildersWorkbenchNext/workbench.db" \
  --confirm
```

**写入对账**(计划 N / 实写 M / 跳过 K,design 七-1 修复轮 Important-4 的三数勾稽):

| 实体 | 计划 | 实写 | 跳过 |
|---|---|---|---|
| project | 3 | 3 | 0 |
| metric | 34(31 条原指标 + 3 条新生北极星行) | 34 | 0 |
| observation | 64 | 64 | 0 |
| issue | 37 | 37 | 0 |
| handoff | 2 | 2 | 0 |
| artifact | 764 | 764 | 0 |

末行标记:`IMPORT_WROTE project=3 metric=34 observation=64 issue=37 handoff=2 artifact=764`。

**sqlite 读回核对**(与上表逐项对上,零硬编):

```sql
sqlite3 "<新库>" "SELECT COUNT(*) FROM project;"                                   -- 3
sqlite3 "<新库>" "SELECT COUNT(*) FROM metric WHERE tier='north_star';"             -- 3
sqlite3 "<新库>" "SELECT COUNT(*) FROM metric;"                                     -- 34
sqlite3 "<新库>" "SELECT COUNT(*) FROM observation;"                                -- 64
sqlite3 "<新库>" "SELECT COUNT(*) FROM issue;"                                      -- 37
sqlite3 "<新库>" "SELECT COUNT(*) FROM handoff;"                                    -- 2
sqlite3 "<新库>" "SELECT COUNT(*) FROM artifact_archive;"                           -- 764
```

**导入流水**(`import_ledger`,只追加表,证明"跑过一次、什么时候跑的、旧库指纹是什么"):

```sql
sqlite3 -header -column "<新库>" "SELECT * FROM import_ledger;"
```

```
id=abb6aec5-88ab-4549-b389-8e142a17def6
legacy_db_path=/Users/gravity/Library/Application Support/BuildersWorkbench/workbench.db
legacy_db_fingerprint=size=2007040;mtime_ns=1786093845658800313
project_count=3 metric_count=34 observation_count=64 issue_count=37 handoff_count=2 artifact_count=764
created_at=1786435172(2026-08-11 15:59:32 本地时间)
```

**观测表触发器复核**(真写完之后故意试一次更新、一次删除,应当被数据库自己拒绝——这是导入之外一次真实的复核机会,design §2.2④ 点名过):

```sql
sqlite3 "<新库>" "UPDATE observation SET raw_value='tamper' WHERE id=(SELECT id FROM observation LIMIT 1);"
-- Error: stepping, 观测只追加,不可修改(observation is append-only) (19)
sqlite3 "<新库>" "DELETE FROM observation WHERE id=(SELECT id FROM observation LIMIT 1);"
-- Error: stepping, 观测只追加,不可删除(observation is append-only) (19)
```

两条都真实报错,数据库级触发器在导入之后照常生效,导入器也绕不过——设计稿要求的这条硬证明成立。

**改名与观测保真的抽查**(第一处硬碰撞的处置结果):aihot 日报的「阶段完成 Issue 数」族改名后 5 行各自的观测数为原型 7 / 构建 6 / 优化 4 / 运营推广 3 / 运维 9,合计 29 条——与设计稿 §2.4 原文「aihot 29 条」逐字对上,证明改名没有丢观测。

---

## 3. 六段点亮验收(aihot 日报)

选 aihot 日报当第一个真实项目(设计稿 §3.1 的推荐,理由:六段里要真数据的段它占得最全)。

**深链启动命令**(`BW_DB` 指向新壳默认库,不需要额外指定路径):

```bash
BW_DB="/Users/gravity/Library/Application Support/BuildersWorkbenchNext/workbench.db" \
  BW_OPEN="aihot 日报" BW_PANEL=hex ./next/target/debug/bw-next
```

**stderr 摘录**(渲染证明,数字从真实 `Vm` 读,不硬编):

```
[BW_OPEN] "aihot 日报" -> panel=Hex 指标=14 观测=60 运行=0 待人处理=15
```

```bash
BW_DB="/Users/gravity/Library/Application Support/BuildersWorkbenchNext/workbench.db" \
  BW_OPEN="aihot 日报" BW_PANEL=attention ./next/target/debug/bw-next
```

```
[BW_OPEN] "aihot 日报" -> panel=Attention 指标=14 观测=60 运行=0 待人处理=15
```

两屏各起一次进程,均在 stderr 打出这一行且进程未崩(用 8 秒存活窗口验证,`kill` 之前没有出现 panic)。截图不是必需项(computer-use/agent 自身截不到真实桌面像素这条结论已在此前多个切片踩实),深链 stderr + sqlite 独立读回就是本记录采用的证据形式。

### 六段逐段核对

| 段 | 设计稿要求 | 深链读数 | sqlite 独立读回 | 判定 |
|---|---|---|---|---|
| ① 项目目标(北极星) | 名字来自真实正本文件,信号可以是「未知」灰 | (含在①段的项目详情里,未单列计数) | 北极星行 `origin='file'`、`target_raw=''`(空);正本文件北极星名字与该行名字逐字相同 | **真数据,信号未知灰**——旧库从未存过北极星目标值,导进来也是空,如实不点灯 |
| ② 五角色责任 | 五张责任卡与项目无关(内核静态元数据);当前这一棒高亮;各棒活数是真的 | `active_stage=1`(原型) | 按 `stage` 分组的活数:原型 8 / 构建 7 / 优化 7 / 运营推广 3 / 运维 9,合计 34 | **真数据**——项目已完整走过一圈五阶段又回到原型,各阶段都挂着历史活是「环」设计的正常现象,不是数据错乱 |
| ③ 引领指标 | 三层卡片来自真实指标行;至少一张不是灰的 | `指标=14`(非停用指标数) | 14 = 16 条(15 原指标 + 1 北极星)− 2 条停用;其中 8 条有观测(全部约 21 天前)、6 条无观测(含北极星);仅 3 条(累计生成日报天数/工作区真实提交数/本周结算活数)同时有 `target_raw` 与观测,能真的算出信号 | **真数据,至少一张不灰**——但那 3 张大概率是「过期琥珀」而不是新鲜绿(见下节),另 2 条本该有目标值的指标(连续产出日报天数/每日命中率)已被停用,不进这 14 张活跃卡片,是正确的停用语义,不是缺陷 |
| ④ 当前 Loop | 至少一条本进程自己开工的运行,走完整生命周期 | `运行=0` | `SELECT COUNT(*) FROM run` 挂在 aihot 下 = 0 | **如实空**——按硬约束本次没有起任何真实会话;起会话受台账第 6 条信任对话框问题阻塞,路线拍板留给用户(设计稿 §4,主控裁决 7);导入本身也不建运行,这一段从设计上就不会被导入点亮 |
| ⑤ 风险与决策 | 交棒流水显示真实记录,带险置顶标红 | (aihot 侧未见,未单独深链核对) | aihot 交棒数 = 0;WorkflowHub 交棒 2 条(1 条带险,`to_stage=3`=优化,恰好等于 WorkflowHub 当前这一棒),是「投影真的会亮」的天然旁证 | **如实空**(选 aihot 时)——它没有交棒记录;要看真数据得换成 WorkflowHub,本记录未对 WorkflowHub 单独起深链(不在设计稿要求的验收范围内),仅用 sqlite 核对了数据形状 |
| ⑥ 交付证据 | 三栏:运行账/观测出处/工作区现状 | `观测=60` | 来源分布 connector 23 / manual 6 / telemetry 31,合计 60;`source_hint`(旧运行编号)60 条全空;工作区 `git status` 干净,HEAD=`7f4d496` | **三栏全真数据,形态不同**——第一栏(运行账)因第④段未点亮同样如实空;第二栏(观测出处)有真数据但旧运行编号全空(旧库这些观测本来就没关联具体运行);第三栏(工作区现状)现场对 aihot 工作区跑 `git status`/`git rev-parse HEAD` 现采,不经导入、不与库比对 |

**「跑完整六段链」的判据**(设计稿 §3.3):六段全部渲染、五段有真数据、第⑤段如实空且有说明、第④段需要一次真会话的完整生命周期——本次**没有**做到第④段,因此按设计稿原话,**这不算「跑完整六段链」**。这不是失败,是主控裁决 10 已经预先定好的口径:

> 「七-主体」(导入器 + 开工闭环 + 六段点亮五段)可先收官;「七-验收里程碑」(六段全链含第④段真会话 + 归档两步)挂在信任问题上,如实标注、不算失败也不冒充完成。

本记录因此把结果拆两半记:**导入 + 五段点亮 = 完成**;**六段全链里程碑 + 归档 = 待信任对话框路线拍板后继续**。

### 待人处理(15 条)的构成

```sql
-- stale(有观测但最新观测距今超过判定新鲜度的窗口,且有真实目标值)
SELECT COUNT(*) FROM metric m JOIN project p ON p.id=m.project_id
WHERE p.name='aihot 日报' AND m.archived_at IS NULL
  AND EXISTS (SELECT 1 FROM observation o WHERE o.metric_id=m.id);        -- 8

-- 无观测(含北极星)
SELECT COUNT(*) FROM metric m JOIN project p ON p.id=m.project_id
WHERE p.name='aihot 日报' AND m.archived_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM observation o WHERE o.metric_id=m.id);    -- 6

-- 评审中的活
SELECT COUNT(*) FROM issue i JOIN project p ON p.id=i.project_id
WHERE p.name='aihot 日报' AND i.status='in_review';                        -- 1
```

`8 + 6 + 1 = 15`,与深链 stderr 打出的「待人处理=15」逐位对上。8 条 stale 指标的最新观测时刻全部落在 2026-07-20~21,距 2026-08-11 约 21 天——**这是切片七-3 侦察报告已经预告过的「过期琥珀」,是真实数据的诚实降级,不是导入器或本次验收出的问题**;界面上这些卡片会显示"数据过期"而不是新鲜绿,不应被误读成「没点亮成功」。

---

## 4. 门禁复核(证明本次操作没有弄坏任何既有防线)

```bash
cd next && cargo fmt --all --check                       # 通过,零 diff
cd next && cargo check --workspace                       # 通过,零 warning(除既有的 block v0.1.6 未来不兼容提示,与本次改动无关)
./scripts/guard-app-layering.sh                           # 通过
./scripts/guard-import-feature-off.sh                     # 通过(app-desktop 依赖图里没有 bw-store-import,产物二进制 strings 核验没有导入 schema 痕迹)
./scripts/guard-kernel-ui-free.sh                          # 通过
./scripts/guard-no-direct-process.sh                       # 通过
./scripts/guard-shell-no-clock.sh                           # 通过(本次只加了文档,没有碰壳源码)
cd next && cargo run -p bw-app --example import_legacy    # 合成老库自测档:44 条断言全过,IMPORT_LEGACY_OK
cd next && cargo run -p bw-app --example issue_kickoff    # 开工闭环 mock 档:88 条断言全过,ISSUE_KICKOFF_OK
cd next && cargo run -p bw-app --example hex_readback     # 六段总控读回档:107 条断言全过,HEX_READBACK_OK
```

五条守卫、三档常绿指挥器(共 239 条断言)、`cargo check`/`cargo fmt` 全部通过——本次是纯粹的真库导入 + 深链读回 + 文档记账,没有改动任何生产代码路径。

---

## 5. 硬约束逐条自查

- **旧库原件全程只读**:第 1 节的三次哈希/大小/修改时刻比对逐字节相同。
- **新壳默认库是唯一写入目标**:真写档的 `--to` 参数就是它,没有任何命令碰过旧库所在目录。
- **没有写任何东西进用户的项目仓**(aihot 等):本次唯一的文件系统写入动作是新库的创建与写入,以及对 aihot 工作区跑的两条只读 git 命令(`git status --short`、`git rev-parse HEAD`)。
- **没有起任何真实 claude 会话**:第④段如实空,原因是台账第 6 条(信任对话框)未解除,路线留给用户拍板,本次没有绕过这条约束去凑一个"看起来点亮了"的假象。
- **数字一律读回**:本记录里出现的每一个数字都能用文中给出的 sqlite 命令原样复核,没有一处是手填或估算。

---

## 6. 下一步

第④段(真会话完整生命周期)与归档旧应用两步,都以设计稿 §4 的信任对话框问题解除为前提——这是主控裁决 7 明言留给用户拍板的事,三条候选路线(甲·预置信任 / 乙·认画面应答 / 丙·让人接一次,外加"集中工作树目录、只信任一次公共父目录"的折中观察)已经在设计稿里摆开,本记录不重复展开,也不代为选择。用户选定路线之后,`--real` 那一档(`cargo run -p bw-app --example issue_kickoff -- --real`)才具备真实通过的条件,归档旧应用第一步(摘门禁/加横幅/打标签)也才会在六段全链点亮之后启动。
