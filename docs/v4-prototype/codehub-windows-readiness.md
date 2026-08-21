# V4 对 codehub 与 Windows:明摆着要修的问题(八条已全部处置)

> **30 秒导读**
>
> - **这是什么**:一份**问题清单 + 处置结果**。先做纯代码级核查,再拿**真跑着的 V3** 逐条比对,只留下代码级就能确定的问题;2026-08-21 又逐条回源码复核了一遍并改掉。
> - **给谁看**:上 Windows / codehub 真环境验收的人,以及以后动这几处代码的会话。
> - **现在还作数吗**:清单本身作数,**但八条都已经改完**(见下表)。核查于 2026-08-21,基线 main `5299334`。
> - **结论**:V3 的实战把大部分担心覆盖掉了。真正要修的是 **V4 拷贝时弄丢的、和 V4 新开的口子**,一共 **8 条,全部不需要真环境就能修** —— 现已全部落地。
> - **边界**:本机是 macOS、没装 codehub-cli、没有 Windows 机器,**修完的东西一样没在真机上验过**。凡是判定「确定有问题」的,依据都是文件行号或 V3 对照,能自己翻回去核;改完之后哪几条还等真机点头,见 §5。
> - **刻意没写的**:详尽的真环境验证流程。用户 2026-08-21 定的调子 —— **只列明摆着的,其余到内部按实战来修**。§5 只留了一小张「第一天先看这几眼」。

## 0 · 八条的处置一览(2026-08-21 复核并改完)

| # | 一句话 | 处置 |
|---|---|---|
| 修-1 | codehub 的 MR / issue 链接必然点不开 | ✅ 已改:改成按平台拼地址,V4 自带那张区别名 → 域名映射;从 SSH origin 推地址的老路整段删掉 |
| 修-2 | Windows 上 gh / cursor-agent / codehub-cli 三项探活恒报假红 | ✅ 已改:找可执行文件按 `PATHEXT` 接扩展名 |
| 修-3 | 探活找 `codehub`,真起的进程是 `codehub-cli` | ✅ 已改:探的名字和真起的进程对齐 |
| 修-4 | 没开内嵌终端时起 claude 的 Windows 路自相矛盾 | ✅ 已改,**但不是报告给的两个修法**:复核发现这条路在 V4 里根本没有调用方,按仓规矩整段删掉(详见该节的「复核修正」) |
| 修-5 | 拷贝残留的死代码 | ✅ 已删,过了编译器,不只信 grep |
| 修-6 | 三处注释与代码不符 | ✅ 已改,另外顺手改了同一份文件里另外两处同病 |
| 修-7 | `pty_smoke` 硬编 `bash` | ✅ 已改:基本场景按平台选 shell;收尾 / 中止两个场景 Windows 上如实报「没实现」,不空跑冒充绿 |
| 修-8 | 打开浏览器会闪一下黑窗 | ✅ 已改:走带 `CREATE_NO_WINDOW` 的那层 |

**复核里发现的、这份稿子原来没提到的两件事**,已分别并进 修-4 与 §3。

代号第一次出现时都带一句人话解释。领域词见仓根 `CONTEXT.md` 词表。

---

## 1 · 先说 V3 的实战覆盖掉了什么

V4 的底座 `v4-engine` 是 2026-08-21 从 V3 的 `bw-engine` 拷来接管的。**V3 在内部真跑着**,所以对每条发现先问:这段代码 V3 有没有?V3 真会走到吗?

用户 2026-08-21 确认了两件事,直接结掉一批担心:

1. **V3 的 claude 内嵌终端,在 Windows 上是真在用的。**
2. **V3 用 codehub 主要就是「关联已有项目」这条路。**

于是:

| 原来担心的 | V3 的实战怎么说 | 还用管吗 |
|---|---|---|
| 内嵌终端的 Windows 后端(ConPTY)没真机跑过 | V4 那份 `pty_backend.rs` 与 V3 那份 **`diff` 无输出、一字不差**,而它是 V3 在 Windows 上天天走的生产路径 | **不用**。见 §4 |
| codehub 的探仓 / 列仓 / 读远端名片 / clone / 开 MR / 合 MR | 整条链就是 V3 关联已有 codehub 项目时走的路,代码 V4 基本原样拷贝 | **不用** |
| 开 MR 时目标分支猜 `master` | V3 那段一字不差,在真 codehub 上跑着 | **不用**,真出事再说 |
| 读远端名片时默认分支用 `main` | 同上,V3 一字不差且在生产上跑 | **不用**,真出事再说 |
| github 合入后没有复查 | V3 那段一字不差,在真 GitHub 上跑了很久 | **不用** |

**剩下的才是真问题**,它们有一个共同点:**要么是 V4 拷贝时弄丢的,要么是 V4 新开的口子** —— 两种 V3 都没走过,所以 V3 的实战给不了它们背书。

---

## 2 · 明摆着要修的八条

全部代码级确定,**不需要 codehub、不需要 Windows 机器就能修**。按该修的顺序排。

### 修-1 · codehub 的 MR / issue 链接必然点不开(V4 弄丢了一张表)

**问题**:链接的根地址由 `bw-v4/src/git.rs:715` 的 `browse_base()` 从仓的 `origin` 推。而 buddy clone codehub 走 SSH(`v4-engine/src/codehub.rs:307` 起,注释说明:codehub 是局域网平台,HTTPS 经代理被拦)。走一遍:

```
origin  = ssh://git@szv-open.codehub.huawei.com:2222/z30026659/maas.git
       ↓ normalize_browse(git.rs:746)
结果    = https://szv-open.codehub.huawei.com:2222/z30026659/maas
```

两处坏了:**端口 `:2222` 留在网址里**(那是 SSH 端口),**主机名也不对** —— `codehub.rs:299-301` 白纸黑字写着 SSH 主机(`szv-open.…`)不等于 API 主机(`open.…`)。

**为什么确定**:V3 根本不这么做。V3 按 provider 拼,并查一张区别名 → 域名的映射表:

```rust
// app-desktop/src/screens/op/issues.rs:25-38
"codehub" => format!("https://{}/{path}/-/merge_requests/{n}",
                     bw_core::codehub_alias_to_domain(host))
```

表在 `bw-core/src/model.rs:1388`,八行:

```
green  → codehub-g.huawei.com
open   → open.codehub.huawei.com
yellow → codehub-y.huawei.com
其它   → 原样返回(已经是域名就直接用)
```

**这是 V4 拷贝时的回退,不是新难题。** 顺带说明 `git.rs:708-710` 那句「codehub 的 host 存的是区的别名,拼不出能点的地址」是错的 —— V3 拼了很久就是这么拼的。

**怎么修**:照 V3 的做法,链接由界面按 provider 拼(`app-shell/src/screens/plan/detail.rs:52-67` 那处),映射表照抄。

**一个硬约束**:**V4 不许依赖 `bw-core`**(V4 对 V3 六个 crate 零依赖是硬规矩)。所以不是 `use bw_core::…`,而是**在 V4 里自带一份同样的八行映射**,符合 V4「领域类型自持」的既定做法。

**多半不用动 `git.rs`** —— 按 V3 的形状,`browse_base` 只管 github 那一侧就够了。

### 修-2 · Windows 上三项探活恒报假红

**问题**:`v4-engine/src/claude_bin.rs:15-22`

```rust
pub fn which_on_path(exe: &str) -> Option<String> {
    std::env::split_paths(&path).map(|dir| dir.join(exe)).find(|p| p.is_file())
}
```

**只按裸名字找,不试 Windows 的扩展名**。Windows 上那些程序叫 `gh.exe`、`cursor-agent.exe`、`codehub-cli.exe`,`dir.join("gh")` 永远找不到。后果落在项目墙那条环境条上(`app-shell/src/bridge/vm_build.rs:397-399`):**这三项在 Windows 上恒报红,哪怕装得好好的**。

claude 那项侥幸没事 —— 它走 `claude_binary_candidates`,里头两条 `%APPDATA%\npm` 路径把标准安装兜住了。

**为什么确定 V3 的实战不能给它背书**:V3 有一模一样的代码,但**只拿它找 `claude`,而且是候选链最后一条兜底**(`bw-engine/src/claude_bin.rs:44`)—— Windows 上前面那两条 npm 路径早命中了,轮不到它。**V4 的环境条是第一个拿它去找 `gh` / `cursor-agent` / `codehub` 的地方。**

**怎么修**:按 `PATHEXT`(或直接试 `.exe`/`.cmd`/`.bat`)逐个试一遍。

**为什么排这么靠前**:`claude_bin.rs:25-30` 那段注释是 2026-08-20 修同一类 bug 的另一半时写的,末尾那句现在反过来打在自己身上 —— **假的红灯比没有灯更坏。**

### 修-3 · 环境条探测的程序名和真正调的对不上

`vm_build.rs:398` 写的是 `which_on_path("codehub")`,而 `codehub.rs` 里 8 处起子进程用的全是 **`codehub-cli`**(`:65`、`:154`、`:188` …)。界面上那格的标签本身就写着「codehub-cli」(`vm_build.rs:420`)。

V3 没有环境条这个东西,所以这也是 V4 新开的口子。

**怎么修(已改)**:改成找 `codehub-cli`。原稿建议「先在真机上 `which codehub` 确认一句,万一安装包也放了 `codehub` 这个别名就不是 bug」——**这个前提站不住**:探活要回答的是「真要起的那个进程在不在」,而真起的是 `codehub-cli`。别名存在与否都不改变结论,别名存在时反而更坏(这格报绿、真要起的那个不在)。

### 修-4 · 没开内嵌终端时起 claude 的那条 Windows 路自相矛盾

`v4-engine/src/interactive_cli.rs:468-471` 的注释说:在 Windows 上从 GUI 程序起控制台进程会**新开一个控制台窗口(用户能看见 agent)**。可紧接着 `:474` 用的是 `crate::win_cmd::tokio_cmd(binary)` —— **这个辅助函数专门就是来把窗口按掉的**(`win_cmd.rs:13` 的 `CREATE_NO_WINDOW`)。

实际会发生的:窗口不出现;stdio 继承自没有控制台的 GUI 父进程;交互式 claude 拿不到任何终端;然后 `:484` 起进程、`:487` 等它退出 —— **当场报错,或者静默挂到超时**。

**复核修正(2026-08-21)**:原稿这里写的「这条路 V4 真会走」**是错的**。顺着调用链走一遍:新壳建 App 时一律 `.with_pty()`(`app-shell/src/bridge/mod.rs:426-431`),于是 `run_issue` 先进内嵌终端分支;工作区目录不在时那条分支返回「没接手」,而阻塞那条路在同样条件下用的是自我标注的替身(`issue.rs:172-180`);六个 headless 例子传的执行器全是替身。**所以 `InteractiveCliExecutor::run_skill` 在 V4 里从第一天起就没有调用方。**

**实际怎么修的**:既不选「让控制台真出来」也不选「承认没窗口、如实报错」—— 那两个修法都得先在 Windows 真机上把控制台与 stdio 的行为摸清楚,为一条没人走的路去猜不值。按仓规矩(「发现过时的实现路径,直接移除」)把这条路整段删了:Windows / macOS / Linux 三个分支、`await_child`、`shell_quote`、墙钟超时字段一起走,`run_skill` 只留一句如实的错误。真跑 claude 的唯一路径是内嵌终端(`run_skill_pty`)—— 这本来就是 `v4-engine` 顶部一直写着的说法。

### 修-5 · 拷贝残留的死代码

V4 拷贝时把一批函数丢下了,壳留在原地。**全仓无人使用**:

| 死掉的东西 | 位置 |
|---|---|
| `CodehubRepoRef` 结构体 | `v4-engine/src/codehub.rs:30` |
| `github::remote_matches` | `v4-engine/src/github.rs:118` |
| `RemoteReconcile` / `RemoteReconcileError` | `github.rs:131`、`:137` —— **孤儿枚举,全仓没有函数产出它们** |
| `github::list_open_issues` + `RemoteOpenIssue` + 解析函数 + 它的测试 | `github.rs:195-260`、`:610` |
| `github::PROJECT_INIT_BRANCH` | `github.rs:271` |

按仓规矩(「发现过时的实现路径,直接移除,不加兼容层」)**该删**。这些在 V3 里都还活着,是 V4 没带过来的那部分的残骸。

### 修-6 · 三处注释与代码不符

| 位置 | 注释说的 | 实况 |
|---|---|---|
| `codehub.rs:359-363` | 有个 `project create --namespace-id …` 的「新建仓」函数 | **这个函数不存在**(V4 两个平台都没带过来,是对称的取舍) |
| `v4-engine/src/remote.rs:40` | codehub 的 `host` 是「green/yellow/inner-source **domain**」 | 是**区的别名**,不是域名。`git.rs:708-710` 那句才对 |
| `v4-engine/src/pty_backend.rs:104-105` | 「只经 `cargo check --target x86_64-pc-windows-gnu **-p bw-engine**` 交叉编译检查」 | 这是 `v4-engine` 的文件,**引用的是另一个 crate 的验证证据**。本次实际验的是 `-p v4-engine`(通过) |

第三条尤其该改 —— 它是「证据标注不实」,正好犯了仓里最在意的那条毛病。

### 修-7 · `pty_smoke` 硬编 `bash`,Windows 上开箱跑不了

`v4-engine/examples/pty_smoke.rs:99-100` 写死了起 `bash -c`。Windows 默认没有 `bash`。这是不碰 claude、不碰网关就能验 PTY 后端的唯一工具。

**怎么修**:按平台选 `bash -c` / `cmd /C`。五行的事。

**优先级已经降了** —— V3 证明 ConPTY 在 Windows 上真能跑,这个工具现在是回归抽查用,不是开路用。

### 修-8 · 打开浏览器会闪一下黑窗

`app-shell/src/chrome/mod.rs:325-327` 的 Windows 分支 `cmd /C start "" <url>` **没走 `win_cmd` 那层**,所以没有 `CREATE_NO_WINDOW`。

小事。链接已被限定必须 `https://` 开头(`:319`),注入风险已经挡住了。

---

## 3 · 不修、但要知道的两件事

**一、走势图上「每周合入的 MR」这条线,codehub 项目是空的。**

`bw-v4/src/trend.rs:158-169` 的 `github_repo()` 对 codehub 返回 `None`,`trend.rs:81-84` 随即写一句「这个项目的远端不是 GitHub,按周查合入数还没接 —— 只画 git 那两条」,**那条线留空、不画,不拿 0 充数**。

这是**缺功能,不是假数据**,处理得诚实。V3 也没有这个能力 —— V3 只有个按状态数总数的 `collect_count`(`bw-engine/src/codehub.rs`),不能按周算合入数。要补是新功能,不在本次清单里。

**二、交叉编译今天过不去,但卡的不是 BW 的代码。**

```
cargo check --workspace --exclude app-desktop --target x86_64-pc-windows-gnu
→ error: failed to run custom build command for `libsqlite3-sys v0.30.1`
  error occurred in cc-rs: failed to find tool "x86_64-w64-mingw32-gcc"
```

`libsqlite3-sys` 自带一份 `sqlite3.c` 要现编,需要 mingw-w64,这台 mac 上没装。链路是 `bw-v4` → `sqlx` → `libsqlite3-sys`。

分开跑才看得出各自状态:

| 目标 | 结果 |
|---|---|
| `cargo check -p v4-engine --target x86_64-pc-windows-gnu` | **✅ 通过(7.84 秒)**,连 `conpty-oxide` 和 `windows-spawn` 一起编出来了 |
| `bw-v4` / `app-shell` | **无证据**,被挡在门外 |

好消息是 **Windows 专属代码全住在 `v4-engine` 里**,而它干净。要补齐另外两个 crate 的证据,`brew install mingw-w64` 即可。

**顺带记一句证明力边界**:`cargo check` 不做链接,全绿也只说明类型和平台分支自洽。

---

## 4 · 减负-14 可以结了

`docs/LEFTOVERS.md` 的 **减负-14** 记的是「内嵌终端的 Windows 后端(conpty-oxide)只交叉编译核对过、没真机跑过」。

**用户 2026-08-21 确认:V3 的 claude 内嵌终端在 Windows 上是真在用的。** 而 V4 那份 `v4-engine/src/pty_backend.rs` 与 V3 那份 **`diff` 无输出、一字不差**(532 行)。

所以那 532 行**早有真实的 Windows 里程**,只是当初没人把这件事记下来。建议把减负-14 结掉,理由写「V3 生产使用即为证据,V4 那份与之逐字相同」。

唯一还留着的小尾巴(§2 修-6 第三行那处注释)已经在 2026-08-21 那次改动里改掉了,并补进了 `diff` 逐字相同这条硬证据。**`docs/LEFTOVERS.md` 里那一行本身没动**,原因见 §6。

---

## 5 · 上真环境第一天,先看这几眼

按用户定的调子,**不写详尽验证流程,其余按实战来修**。这里只留几条一眼能看出真假的:

**codehub**(装了 codehub-cli 的机器):

| 看什么 | 看到什么算有问题 |
|---|---|
| `which codehub` / `which codehub-cli` | 只有 `codehub-cli` 有 → 修-3 坐实 |
| 关联一个已有 codehub 仓,`sqlite3 <库> "SELECT provider,remote_host,remote_path FROM project;"` | `provider` 不是 `codehub`,或 `remote_host` 不是 `open`/`green`/`yellow` |
| 那个仓的网页域名,和修-1 那张映射表对一对(现在这张表在 `bw-v4/src/model.rs::codehub_alias_to_domain`) | 对不上 → **改那张表**,别改拼装逻辑 |
| 点计划屏详情里的 MR / issue 链接 | 打不开 → 修-1 没修干净(**改完之后预期是能打开**) |

**Windows**:

| 看什么 | 看到什么算有问题 |
|---|---|
| 项目墙那条环境条 | 明明装了 gh / codehub-cli / cursor-agent 却报红 → 修-2 / 修-3 没修干净(**改完之后预期是绿**) |
| 内嵌终端里 ▶跑 一件活 | 起不来 → 这条 V3 在用,起不来说明 V4 这边接线接坏了,不是后端问题 |
| 拖动计划屏那六列 | 拖不动 → `app-shell/src/main.rs:41` 那行没生效(V4A-12) |

---

## 6 · 为什么没往 `docs/LEFTOVERS.md` 加行

原稿这里备了七行「兼容-1 … 兼容-7」等着粘进 `docs/LEFTOVERS.md` 的「当前开着」表,并准备在 `docs/code-schemes.md` 开一个 `兼容-N` 系列。**这两件都没做,是有意的:**

- 用户 2026-08-21 明确要求 `docs/LEFTOVERS.md` **往下砍,不要继续长**(它已经 535 行)。而这八条当场全改完了,本来也不该变成欠账;剩下要真机点头的那部分写在 §5 与那次改动的 PR 描述里,够用。
- `兼容-N` 这个代号系列**没有开**,所以也不用登记。`docs/code-schemes.md` 是防同字母撞车的登记表,不是欠账表 —— 不开新系列就不该往里写。
- **减负-14 那一行也没动。** §4 的判断复核过了(V4 那份 `pty_backend.rs` 与 V3 的 `diff` 无输出,532 行逐字相同),但 `docs/LEFTOVERS.md` 是多窗口热文件,这次只把结论写在这儿和 `pty_backend.rs` 的注释里,谁在改那份文件谁顺手结。

**改完之后还没有证据的,只剩两类**,都在 §3 与 §5 里写着:`bw-v4` / `app-shell` 的 Windows 可编译性(交叉编译被 `libsqlite3-sys` 缺 mingw-w64 挡住,只有 `v4-engine` 单独验过),以及所有 Windows / codehub 行为的真机效果。
