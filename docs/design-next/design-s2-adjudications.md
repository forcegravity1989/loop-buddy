> **30 秒导读(归档横幅,2026-08-11)**:本文是 vNext 切片设计事实源的**归档正本**,原生于执行会话的暂存目录,切片五收官时归档进仓。文末(或配套文件)的「主控裁决」是定案;实施中的偏差以任务报告与 commit 正文为准。现状以 `plan/23-opc-stitching-rebuild.md` 进度实况表为准。

# 切片二设计 12 条开放问题 · 主控裁决(2026-08-10)

配套设计稿:design-s2-connector.md(opus 产出)。裁决原则:做减法、诚实优先、避免循环依赖。

1. **workspace.rs 落哪** → 落 `bw-connector/src/upstream/workspace.rs`,单副本。侦察已证 evidence.rs 自包含不需要它,所以不存在「两副本」问题;避免 connector→engine 循环。
2. **建仓/克隆/列仓** → 接入期自由函数,放 bw-connector crate 内(禁令禁的是编排层直接进程调用,函数在连接器 crate 内不算绕过)。不开第五能力。
3. **checks** → 切片二不做(超纲);六段「交付证据」真需要时(切片五前)再包 `gh pr checks`。
4. **create_issue 防重** → 接受不对称:open_change 真防重(分支锚点),create_issue 的 IdemKey 仅作日志追溯,文档注释如实标注。
5. **IdemKey 落表** → 首版不落 connector_write_log 表(减法);切片四做重启清理时若发现真需要再评,届时走 schema 双守卫。
6. **stderr→错误分类映射** → 允许,约束三条:集中在每适配器一个 `classify()` 函数、每条映射注明验证日期、映射不到的落 UpstreamRejected 原文透传。这不是被禁的「字符串分支」(那条禁的是按 connector 身份分派)。
7. **探活结果落列** → 不落列。内存缓存 + 待人处理投影;宁可界面慢,不留过期绿。界面性能问题真出现再议。
8. **一项目多仓连接器** → 保持 Ambiguous 报错(保守正确);主/副机制等真实需求出现。
9. **connectors.toml 解析器** → 放 bw-connector(script_source.rs,生产 ConnectorEntry 的地方)。
10. **agentcli 的 Probe** → `--version` 通过只报「已安装」档,不冒充已连接;真实连接状态由第一次真实运行回填。不起最小会话探活(花钱)。
11. **通信能力占位** → 不占位(空枚举项诱导填充,违反减法)。
12. **CollectOut 返回形状** → 一个方法 + enum 返回,不拆两个方法(能力矩阵不膨胀)。

设计稿中已拍板采纳的关键决定(切片二简报直接照用):新薄 crate `bw-connector` + 每适配器一个 feature;基座 trait + `as_probe/as_execute/as_collect/as_issue_ops` 上转;`WriteOutcome::{Created,AlreadyExisted}` + read-before-write;`ExecState` 无 Done 档(类型断路);`guard-no-direct-process.sh` 新守卫;`probe_all.rs` 指挥器必须含一条故意失败路径。

## 追记(2026-08-10,opus 评审后设计修正,已裁决入 task-s2a-fixlist.md)

设计稿本身被评审证伪/修正的点,后续切片以修复后代码为准:guarded 是 pub(§4 胜 §6);路由方法返回持有句柄 Vec<(ConnectorEntry, Arc<dyn Connector>)>(并发探活需要 'static);超时五档收进 OpClass 枚举;IdemKey/RequestId 构造器收口;Ok<T> 改名 CallOk<T>;ProbeReport.reachable 已删;OpenChangeReq 加 base,契约冻结点=二B 收编完成。遗留给切片三设计:ExecState::Finished{ok:bool} 的布尔盲化要不要改枚举。
