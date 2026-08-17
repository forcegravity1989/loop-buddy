# Builders' Workbench(BW / loop-buddy)

> **30 秒导读**:这是仓库的总入口——它是什么、怎么跑起来、文档在哪。给第一次 clone 下来的人看。**现在作数**。给 AI 会话的工作纪律在 [`CLAUDE.md`](CLAUDE.md);开发者操作手册在 [`DEVELOPMENT.md`](DEVELOPMENT.md);全仓文档地图在 [`docs/README.md`](docs/README.md)。

## 是什么

单人构建者的 Rust 原生桌面工作台(Dioxus 0.7 / wry WebView,macOS + Windows)。产品命题一句话:**用 AI 时代的方式,一步步把一个项目的管理体系搭起来;走完,你拥有一套可复制的项目管理方法,而不只是一块看板**。完整拆解见 [`plan/07-product-proposition.md`](plan/07-product-proposition.md)。

主环(用户在这里真干活):**项目墙 → 建/接项目 → Issue 看板 → ▶跑(内嵌终端里的真实 `claude`)→ 评审 → 人点「完成」→ 一键蒸馏成技能**。四条产品铁律:「完成」永远由人点;健康信号只能从真实数据推导(没数据就是灰,绝不假装绿);同一件活绝不重复记账;定时任务只自动**建**活、绝不自动**完成**活。

三个名字是一件东西:仓库真名 **loop-buddy**(GitHub);产品名 **Builders' Workbench(BW)**;**buddy** 是产品里那个 AI 队友/程序的自称。

## 怎么跑

```bash
# 需要 Rust 1.82+;桌面壳另需 Dioxus 0.7 的系统依赖(macOS 自带 WebKit,Windows 用 WebView2)
cargo check -p bw-app                       # 日常最快:编译内核+应用,不编 Dioxus
cargo run -p app-desktop                    # 启动桌面应用(默认库在 ~/Library/Application Support/BuildersWorkbench/)
BW_DB=/tmp/bw.db cargo run -p app-desktop   # 用一个临时空库启动

# 不开界面证明内嵌终端在这台机器上能跑(macOS/Linux/Windows 都走同一条路径):
cargo run -p bw-engine --example pty_smoke
```

▶跑 需要本机装好 `claude` CLI(`BW_CLAUDE_BIN` 可指定路径)。真跑受 claude 的信任对话框与网关状态影响,那不是 BW 的门禁。

## 仓库结构

```
crates/         bw-core(领域内核,wasm32 可编)· bw-engine(执行器/PTY/证据采集)· bw-store(SQLite)
                · bw-app(编排大脑,Command/Event 总线)· ui(ViewModel)· app-desktop(Dioxus 壳)
plan/           现役计划与规范 7 篇(README 说明哪些作数)
docs/           文档地图 docs/README.md · 当前迭代线 v1~v3-prototype/ · 运行时资产 buddy/ skills/
                · 缓做清单 BACKLOG.md · 历史档案 archive/
iterations/     PRACTICE-buddy.md(实践日志)
e2e/ examples/ scripts/   验收流考卷与种子库 · 样板间库与 vendor 技能库 · 门禁与联调脚本
```

架构、门禁命令、headless 例子清单、验证方式见 [`DEVELOPMENT.md`](DEVELOPMENT.md);领域词表见 [`CONTEXT.md`](CONTEXT.md);现在在做什么见 [`docs/v1-prototype/`](docs/v1-prototype/) → [`docs/v2-prototype/`](docs/v2-prototype/) → [`docs/v3-prototype/`](docs/v3-prototype/)。

## 提交前

门禁与 CI 完全一致,一条不能少(命令见 `DEVELOPMENT.md`「常用命令」);commit 标题必须让不查文档的人看懂做了什么;数字一律 `sqlite3` 读回、不硬编。
