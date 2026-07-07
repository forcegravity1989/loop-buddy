# Builders' Workbench — 开发指南 (P0 + P1)

Rust 桌面应用,按 [`plan/00-PLAN.md`](plan/00-PLAN.md) 路线图实现。当前进度:**P0 · 基座** 与 **P1 · 架构脊椎** 均已落地并通过出口测试。**M1(走通脊椎)核心已证**:架构、Command/Event、度量派生、本地持久化端到端跑通。

## 工作区布局

```
crates/
  bw-core/      ✅ 领域内核 + 度量派生链(P0 主交付)
  bw-engine/    ✅ Executor 契约 + MockExecutor + 一致性测试套件(P1)
  bw-store/     ✅ SQLite 持久化 + recompute_signals 唯一信号写入(P1)
  bw-app/       ✅ AppState + Command/Event 总线 + dispatch 用例 + subscribe(P1)
  ui/           ◐  纯函数 selector 子集(signal_color/phase_style/sparkline/overview_attention…;buildApp 全量移植在 P2/P3)
  app-desktop/  ◐  Dioxus 0.7 桌面壳(P0 = throwaway "hello signals" 爬坡;真壳 P2 起)
  app-web/      —  非成员,"以后也许" 留口(Tier E)
```

`default-members` 只含无头内核 + ui,故日常 `cargo test` / `cargo check` **不编译 Dioxus**,快且稳。桌面壳需显式 `-p app-desktop`。

## 常用命令

```bash
cargo test                       # 内核 + selector + compile-fail doctests(默认成员)
cargo test -p bw-core            # 只测派生链
cargo run -p app-desktop         # 跑 Dioxus 爬坡 app(或 dx serve --package app-desktop)

# CI 同款门禁(本地可跑):
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features   # Web 留口保活
./scripts/guard-kernel-ui-free.sh                                             # 内核禁依赖 UI
```

## P0 两条不可妥协 —— 已钉进类型

1. **UI 无关内核**(plan `00 §3`①):内核只依赖 `serde/time/uuid/thiserror`;`guard-kernel-ui-free.sh` 在 CI 拦截任何 dioxus/tauri/wry/leptos 渗入;wasm32 check 免费保活 Web。
2. **健康永远 derive,绝不手设**(plan `§2.5`):`Signal{Green,Amber,Red,Unknown}` 只能经封口的 [`Derived<Signal>`](crates/bw-core/src/derive/sealed.rs) 进入缓存字段;`Derived::seal` 是 `pub(in crate::derive)`,**全 workspace 无法在 derive 外构造健康信号** —— 由 `sealed.rs` 上的两个 `compile_fail` doctest 在 `cargo test` 中证明。

## 度量派生链(L0→L6)落点

| 层 | 实现 | 文件 |
|---|---|---|
| L1 归一标量 + 过期 | `measure()` | [`derive/measure.rs`](crates/bw-core/src/derive/measure.rs) |
| L2 目标 mini-DSL | `parse_target()`(`≥5 ≤24h <800 100% 7/7 清零 全覆盖 ↑ 跟踪`) | [`derive/target.rs`](crates/bw-core/src/derive/target.rs) |
| L2 值-比-目标 | `evaluate_metric()`(Missing→Unknown;stale→Amber 封顶) | [`derive/eval.rs`](crates/bw-core/src/derive/eval.rs) |
| L4/L6 worst-of | `reduce_worst_of()`(含 Unknown 档,空→Unknown) | [`derive/eval.rs`](crates/bw-core/src/derive/eval.rs) |
| L5 环节 health | `OpStage::health()` 纯投影 | [`model.rs`](crates/bw-core/src/model.rs) |
| L6 项目信号 | `Project::derive_signal()` | [`model.rs`](crates/bw-core/src/model.rs) |

amber 带按指标可配 `RelPct | AbsPoints`:`99.9%` 可用率必须用 `AbsPoints`,否则扁平 10% 会把 ~90% 误判为绿(`availability_band_needs_abs_points` 测试钉死此陷阱)。

## P1 架构脊椎 —— 关键落点

- **命令进、事件出**:[`bw-app`](crates/bw-app/src/lib.rs) `App::dispatch(Command)` / `subscribe()`(tokio broadcast)/ `snapshot()`。UI 永不直接碰 store/engine。
- **Executor = 冻结的跨团队契约**:[`bw-engine`](crates/bw-engine/src/lib.rs) `Executor` trait + `MockExecutor` + [`contract::check`](crates/bw-engine/src/contract.rs) 一致性套件。同事的真实现过同一套测即可热插拔(Tier C),`App<E>` 泛型零改动。
- **值唯一诞生地 = append-only `observation`**;**信号唯一写入者 = `recompute_signals`**:[`bw-store`](crates/bw-store/src/sqlite.rs) 无 `set_signal`,所有 `signal/hit` 列只由 recompute 调 `bw_core::derive` 写。每表 `updated_at + rev` 留 sync 口。
- **P1 出口闸门**:[`spine.rs`](crates/bw-app/tests/spine.rs) headless 集成测试 —— 建项目→7 步向导(录 Manual 值)→CompleteWizard→RunWorkflow(mock)→SendMessage→落 SQLite→**杀进程重开**,断言数据全在且每个持久化信号 == 独立 `bw_core` 重算(绝不编造)。

## 下一步:P2 · 纵切 UI

把真 Dioxus 窗口架到已证脊椎上,验证最险的 **Event→Signal 桥**:设计系统地基 + CJK 字体 `asset!()` bundle + 项目墙 + 完整 7 步向导 + 一个运营 panel-view(`showProgStage`)。出口:mac 上启动→建项目→走完向导录真值→落运营视图,每个信号点/sparkline 从录入值 derive;退出重开还原;Event↔Signal 桥无泄漏/无过度渲染。
