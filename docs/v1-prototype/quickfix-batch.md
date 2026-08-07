# V1 Quickfix 批次 · 4 条小 BUG

> 2026-08-07 晨会遗留分析后,把 4 条好处理的小 BUG 在主会话一次性清掉。每条不动铁律,带事实源。设计决定记此,不另开 issue 设计 md。

## 1. W2-4 · guide m6 把 P10 状态写串了

**现象**:`docs/guide/buddy-guide.html` m6 机制章有一行「总览手填入口:BizMetricCard / 北极星灰卡**无** RecordInline(P10 未关)」。但 P10 已落地(`299a96e`:manual 时 BizMetricCard 内嵌 `RecordInline`,op.rs:2347),这行过时。

**改法**:改成「已接:manual 指标卡内可手填(P10 已落地)」。

**事实源**:`docs/guide/buddy-guide.html:861`、`crates/app-desktop/src/screens/op.rs:2347`(BizMetricCard manual 挂 RecordInline)。

## 2. W3-6 · 工程统计三件套(stats trio)是死字段

**现象**:总览 v2 已拿掉 stats trio 显示(W3 决议),但 `OpVm.stats: StatCardsVm` 仍在 kernel.rs 声明+填充,op.rs 零引用——死字段。

**改法**:整删 `ui/vm.rs` 的 `StatCardsVm` + `stat_cards` 函数;删 `kernel.rs` 的 `OpVm.stats` 字段 + 填充调用 + 赋入。

**事实源**:`crates/ui/src/vm.rs:517/527`、`crates/app-desktop/src/kernel.rs:205/1212/1283`(op.rs grep `\.stats` 零命中)。

## 3. W3-8 · weekly delta 伪「没变」

**现象**:`weekly_spark` 做 carry-forward(空周继承旧值保折线连续),`weekly_delta` 读末两桶算 delta。本周没采但 8 周窗内有旧数据时,末周桶被 carry-forward 填满 → delta 算成 `0.0`,渲染「→ 0.0」像「没变」,实为「本周没采」。

**改法**:`ui/vm.rs` 加 `pub fn last_week_has_real_obs(obs, now_unix) -> bool`(末周桶有无真观测,不算 carry-forward);`kernel.rs:1000` delta 改成「末周无真观测 → `None`」。op.rs 不动(delta 渲染已支持 `None → "—"`)。

**事实源**:`crates/ui/src/vm.rs:273/307`、`crates/app-desktop/src/kernel.rs:999-1000`、`crates/app-desktop/src/screens/op.rs:2196`。

## 4. osascript 假装跑完(违反读回为证)

**现象**:`interactive_cli.rs` macOS 分支(`#[cfg(target_os="macos")]`)用 osascript 启 Terminal 后,`tokio::time::sleep(self.timeout)` 睡满 1 小时,然后返回 `completed: true`——实际没等 claude 退出、没验证,谎报完成,违反「读回为证」铁律。

**改法**:返回 `completed: false` + summary「未验证:osascript 启 Terminal 后拿不到 claude 句柄,无法等待退出,请人工在 Terminal 确认」。已核实后果:`lib.rs:2699` completed=false 走 `Err` 分支,issue 停 InProgress 不自动 Done——诚实。**只动 macOS 分支,Windows 主平台不受影响**。

**事实源**:`crates/bw-engine/src/interactive_cli.rs:615-640`、`crates/bw-app/src/lib.rs:2699`(completed 消费)。

---

## 验证

改完跑门禁 6 步 + `cargo test --workspace --exclude app-desktop`,重打 bundle。
