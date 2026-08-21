# V4 对 codehub 与 Windows 的兼容性:今天真做到哪

> **30 秒导读**
>
> - **这是什么**:一次纯代码级的核查报告。回答两个问题 —— buddy V4 对 codehub(公司内部的 GitLab 系代码平台)支持到什么程度、对内部 Windows 支持到什么程度。
> - **给谁看**:准备拿真环境验证的人,以及接手修这些问题的下一个会话。
> - **现在还作数吗**:作数。核查于 2026-08-21,基线是 main `5299334`。
> - **一句话结论**:两块都是「代码写了、V4 这边真环境一次都没验过」。核查中查出 **9 个真问题**,其中 3 个是「一到真环境必然出事」,不是理论风险。
> - **2026-08-21 补充(重要)**:V3 是**真的在内部跑着的**,所以 V3 的同一段代码就是**实战证据**。补比了一轮,结论有三处实质变化 —— 见 §0.1。最要紧的一条:**修 codehub 链接需要的那份「一手信息」,其实一直躺在本仓里**(V3 有一张区别名 → 域名的映射表,V4 拷贝时弄丢了)。
> - **重要边界**:本机是 macOS,**没有装 codehub-cli,也没有 Windows 机器**。所以这份报告里**没有任何一条是实机验证过的**。凡是写「已验证」的,验的都是本机能验的东西(编译、代码可达性),会逐条注明验的到底是什么。

本文里出现的代号,第一次出现时都会带一句人话解释。看不懂的领域词见仓根 `CONTEXT.md` 的词表。

---

## 0 · 核查是怎么做的(以及它证明不了什么)

| 手段 | 做了 | 证明了什么 | **证明不了什么** |
|---|---|---|---|
| 读代码 | 全部 | 哪条路会走到哪、哪些分支从没人走 | 跑起来对不对 |
| 交叉编译 | `cargo check --target x86_64-pc-windows-gnu` | 见 §2.1 —— 只过了一部分 | 链接期、运行期一概不证明(`check` 不做链接,本机也没有 mingw 链接器) |
| 连真 codehub | **没做** | — | 全部 |
| 上 Windows 真机 | **没做** | — | 全部 |
| **读 V3 的同一段代码** | 补比了一轮 | **V3 在内部真跑着** —— 同一段代码在 V3 里跑过,就是实战证据;V4 拷贝时丢了什么,也一比就知道 | V3 没走到的分支,照样没有证据 |

「读回为证」这条纪律在这次核查里的落法:每个结论后面都跟着**文件行号**,拿着行号能自己翻回去核对。不能靠行号核对的,一律写成「待验证」,不写结论。

### 0.1 拿 V3 当证据:同一段代码,V3 到底跑没跑过

V4 的底座 `v4-engine` 是 2026-08-21 从 V3 的 `bw-engine` 拷来接管的。**V3 在内部真跑着**,所以对每一条发现,先问两个问题:**这段代码 V3 有没有?V3 真会走到它吗?**

- 「V3 有、而且真会走到」→ 这条大概率**不是 bug**,是已经被实战磨过的写法。
- 「V3 有、但从来走不到」→ V3 的实战**证明不了它**,发现依然成立。
- 「V3 有、V4 弄丢了」→ 这是**拷贝时的回退**,最该修,而且**修法现成**。

比完的结果:

| 发现 | V3 里的同一段代码 | V3 真会走到吗 | 结论怎么变 |
|---|---|---|---|
| ① codehub 链接坏 | **V3 完全不这么做** —— 它按 provider 拼,并用一张区别名 → 域名的映射表(`bw-core/src/model.rs:1388`) | **会,而且是生产路径** | **变严重,但也变好修**:这是 V4 拷贝时的回退,不是未知难题。**修法在仓里现成** |
| ② 探测名 `codehub` vs `codehub-cli` | V3 **压根没有这条环境条** | 走不到 | 不变。**V4 独有的新暴露面**,V3 的实战证明不了它 |
| ③ 开 MR 目标分支兜底 `master` | 一字不差 | **会** | **降级**:这条在真 codehub 上跑过,没听说出事 |
| ④ 读名片默认 ref 用 `main` | 一字不差(连那段「分支找不到 ≠ 没这份文件」的区分都一样) | **会** | **降级为待确认** —— 见下方问题清单 |
| ⑤ github 合入无读回 | 一字不差 | **会,而且是生产路径** | **降级**:在真 GitHub 上跑了很久,没听说假「完成」 |
| Windows · 找程序不试扩展名 | 一字不差 | **走不到** —— V3 只拿它找 `claude`,而且是最后兜底,Windows 上前面那两条 npm 路径已经命中 | 不变。**又一处 V4 独有的新暴露面** |
| Windows · 起 claude 那条自相矛盾 | 一字不差 | **大概率走不到** —— V3 那边同样只在「没开内嵌终端(指挥器/headless)」时才走(`bw-app/src/issue_run.rs:836-851`) | 不变 |
| Windows · 内嵌终端(ConPTY) | **一字不差,`diff` 无输出** | **是 V3 的生产路径** | **可能大幅降级** —— 取决于一个我答不了的问题,见下 |

**最要紧的那条展开说 —— V4 把一张表弄丢了:**

V3 拼 codehub 的 MR 链接是这样的(`app-desktop/src/screens/op/issues.rs:25-38`):

```rust
"codehub" => format!("https://{}/{path}/-/merge_requests/{n}",
                     bw_core::codehub_alias_to_domain(host))
```

而那张映射表就在 `bw-core/src/model.rs:1388`,一共八行:

```
green  → codehub-g.huawei.com
open   → open.codehub.huawei.com
yellow → codehub-y.huawei.com
其它   → 原样返回(已经是域名就直接用)
```

**所以我在初稿里写的「修它需要真环境的一手信息」是错的** —— 那份信息一直在仓里。V4 改成从 SSH 地址推,不但推错,而且是**主动放弃了一份已经跑通的映射**。`git.rs:708-710` 那句「codehub 的 host 存的是区的别名,拼不出能点的地址」也因此站不住:**V3 拼了两年就是这么拼的。**

有一个约束要注意:**V4 不许依赖 `bw-core`**(V4 对 V3 六个 crate 零依赖是硬规矩)。所以修法不是 `use bw_core::…`,而是**在 V4 里自带一份同样的八行映射**。这符合 V4「领域类型自持」的既定做法。

### 0.2 我答不了、要你确认的三个问题

这三个问题的答案会直接改掉上面几条的结论,**我没有渠道知道**:

1. **V3 在 Windows 上真的被用过吗?用的时候开内嵌终端了吗?**
   —— 如果是,那么 `pty_backend.rs` 那 532 行(V3/V4 **一字不差**)就有真实的 Windows 里程,**减负-14 基本可以结**,§3.2 第 3–5 条也就从「必须验」降成「回归抽查」。如果 V3 只在 macOS 上用,那这条一点没变。
2. **V3 接入 codehub 仓时,有人手打过仓地址吗?那个仓的默认分支是 `master` 吗?**
   —— 这条能判定发现 ④ 是真摩擦还是纸上谈兵。V3 和 V4 这段代码一模一样,V3 没出事就说明大家都是从列表里点的。
3. **V3 里 codehub 项目的 `remote_host` 字段,库里存的到底是 `open` 这种别名,还是完整域名?**
   —— 两种 V3 都吃得下(映射表的兜底分支是原样返回)。但 V4 的接入屏**只会写别名**(`onboard/mod.rs:129-140`),所以 V4 补映射表时必须按别名来。你要是能 `sqlite3` 读一下 V3 真库,一句话就定了:
   ```
   sqlite3 <V3库> "SELECT provider, remote_host, remote_path FROM project WHERE provider='codehub';"
   ```

---

## 1 · codehub 这一块

### 1.1 V4 真会走到的 codehub 路径,一条不落

底座 `v4-engine` 的 `codehub.rs` 一共对外给了 7 个能力。**7 个全都被 V4 用到了**,没有一个是摆设:

| 能力(人话) | 底座函数 | V4 从哪里调进来 | 经不经 provider 分发 |
|---|---|---|---|
| 看一眼这个仓在不在 | `codehub::probe` | `app-shell/src/bridge/mod.rs:339` | 否,直调 |
| 列「我账号下的仓」 | `codehub::list_repos` | `app-shell/src/bridge/mod.rs:283` | 否,直调 |
| 不 clone 就读远端的 `.bw/project.toml` | `codehub::fetch_project_toml` | `app-shell/src/bridge/mod.rs:323` | 否,直调 |
| 把仓 clone 到本机 | `codehub::clone_repo` | `bw-v4/src/app/project.rs:168` | 否,直调 |
| 在已推上去的分支上开 MR | `codehub::create_mr_on_branch` | `bw-v4/src/app/worktree.rs:196` | **是** |
| 查这个分支上有没有开着的 MR | `codehub::open_mr_for_branch` | `bw-v4/src/app/ops.rs:231` | **是** |
| 合入 MR | `codehub::merge_mr` | `bw-v4/src/app/ops.rs:242` | **是** |

**两条路的分野**说清楚:

- **经分发的**(3 条):走 `v4_engine::remote::Remote` 这个枚举。调用点只调方法、不管对面是哪个平台,平台分支集中在 `crates/v4-engine/src/remote.rs` 一处。这是设计想要的样子。
- **直调的**(4 条):调用点自己写 `if provider == "codehub" { ... } else { ... }`。分散在两个 crate 的 4 个地方。

这个分野本身值得记一笔:**接入阶段的 4 件事(探仓、列仓、读名片、clone)全在分发体系之外**。今天没出错,是因为每个调用点都老老实实自己分了支;但「加一个新平台只改一处」这条设计承诺,对这 4 条不成立。归入待拍板项(见 §4.2)。

### 1.2 逐条比对:同一件事,两个平台行为一致吗

已知那条(走势图)先结论:**属实,但处理得诚实**。`bw-v4/src/trend.rs:158-169` 的 `github_repo()` 对 codehub 返回 `None`,`trend.rs:81-84` 随即写一句「这个项目的远端不是 GitHub,按周查合入数还没接 —— 只画 git 那两条」,那条线**留空、不画,不拿 0 充数**。这是「没数据就说没数据」,是缺功能,不是假数据。

除此之外,**又查出 5 处不一致**:

| # | 不一致在哪 | github 侧 | codehub 侧 | 后果 |
|---|---|---|---|---|
| ① | **MR / issue 链接的地址** | 能点开 | **点不开** | 见下,最严重的一条 |
| ② | **环境探测找的程序名** | 找 `gh`,调 `gh` ✓ | 找 `codehub`,调的却是 `codehub-cli` | 探活可能恒报红 |
| ③ | **开 MR 时的目标分支** | 不猜(`gh` 自己从仓推) | 猜:推不出就退回 `master` | 猜错则开 MR 失败 |
| ④ | **读名片时的默认分支** | 空则用 `main` | 空则用 `main`,但内部仓常是 `master` | 手打地址那条路大概率读不到 |
| ⑤ | **合入后有没有复查** | **没有** | 有,复查 MR 状态真变 `merged` 才算成功 | github 侧有假「完成」的口子 |

逐条展开:

**① MR / issue 链接对 codehub 必然是坏的(最严重)**

链接的根地址由 `bw-v4/src/git.rs:715` 的 `browse_base()` 从仓的 `origin` 地址推,再由 `git.rs:746` 的 `normalize_browse()` 归一化。而 buddy clone codehub 仓走的是 SSH(`crates/v4-engine/src/codehub.rs:307` 起,注释写明:codehub 是局域网平台,HTTPS 经代理被拦,所以走 SSH)。

把这两件事接起来,拿 `codehub.rs:296-303` 注释里的真实地址形状走一遍:

```
origin  = ssh://git@szv-open.codehub.huawei.com:2222/z30026659/maas.git
       ↓ normalize_browse:剥 "ssh://git@"、去掉尾巴 ".git"
结果    = https://szv-open.codehub.huawei.com:2222/z30026659/maas
```

两处坏了:

1. **端口 `:2222` 原样留在了网址里**。那是 SSH 端口,浏览器打不开。
2. **主机名不对**。`codehub.rs:299-301` 白纸黑字写着 SSH 主机(`szv-open.codehub.huawei.com:2222`)**不等于** API 主机(`open.codehub.huawei.com`);网页主机是哪个,这份代码里没有任何地方知道。

于是 `app-shell/src/screens/plan/detail.rs:52-67` 拼出来的 `.../-/merge_requests/<号>` 虽然形状对(GitLab 那一系确实在 `/-/` 底下),**地址整个是错的**。GitHub 侧不受影响:`gh` clone 出来的 origin 要么是 HTTPS、要么是 `git@github.com:owner/repo.git`,都没有端口,主机名也就是 github.com。

**V3 怎么做的(实战证据)**:V3 根本不从 origin 推,而是按 provider 拼、用区别名查一张域名映射表(`app-desktop/src/screens/op/issues.rs:25-38` + `bw-core/src/model.rs:1388`)。**这是 V4 拷贝时的回退,不是新问题**,详见 §0.1。

`git.rs` 是另一个窗口正在动的文件,**本次只报不改**。但要说清楚:**修它未必要动 `git.rs`** —— 照 V3 的做法,链接由界面按 provider 拼,`browse_base` 只管 github 那一侧就行。

**② 探测找 `codehub`,实际调 `codehub-cli`**

`app-shell/src/bridge/vm_build.rs:398` 写的是 `which_on_path("codehub")`,而 `codehub.rs` 里 8 处起子进程用的程序名全是 `codehub-cli`(如 `codehub.rs:65`、`:154`、`:188`)。界面上那个探测格的标签本身就写着「codehub-cli」(`vm_build.rs:420`),等于**标签和它真正找的东西对不上**。

如果 codehub-cli 的安装包不额外放一个叫 `codehub` 的别名,那么**在一台装好了 codehub-cli 的机器上,环境条依然报红**。这台机器上没装,**验不了**,列进 §3 清单第 1 条。

**③ 目标分支:一边猜,一边不猜**

`codehub.rs:110-121` 先问 `git symbolic-ref refs/remotes/origin/HEAD`,问不出就退回 `"master"`。github 侧的 `create_pr_on_branch`(`github.rs:307`)**压根不传目标分支**,让 `gh` 自己从仓里推。

猜错的后果是如实失败(codehub-cli 会报 branch not found),不会静悄悄地开错 MR —— 这一点是好的。但这是两套不同的可靠性:一边零猜测,一边有个可能猜错的兜底。

**④ 读名片的默认分支是 `main`,而内部仓常是 `master`**

两侧都一样:ref 传空就用 `"main"`(`codehub.rs:377-381`、`github.rs:532-536`)。**代码自己就承认内部仓不是这样** —— `codehub.rs:97` 的注释原话说 maas 那个仓是 `master`。

界面上有两条路进来:

- 从列表里点一个仓 → `onboard/mod.rs:495` 把该行真实的 `default_branch` 传进去。**没问题**。
- **手打仓地址** → `onboard/mod.rs:276` 传的是空串 → 退回 `main` → 一个默认分支是 `master` 的 codehub 仓会走进 `codehub.rs:410` 的「branch not found」分支 → 报 `Err`。

**V3 那份一字不差**,连下面那段区分逻辑都一样,而 V3 在真 codehub 上用着 —— 所以这条更可能是「大家都从列表里点,没人手打」,而不是天天在炸(§0.2 问题 2 能定这件事)。好消息是这里**没有把「分支找不到」和「没这份文件」混成一件事**(两处注释都专门解释了为什么不能混),所以人看到的是一句「没查成」,不是假的「首来者,请填」—— 不会误导人去覆盖同事已经写好的名片。坏消息是接入体验会卡住,而且 codehub 的命中率比 github 高得多。

**⑤ 合入后的复查,只有 codehub 做了**

`codehub.rs:252-291` 有一段很硬的读回:因为 2026-07-31 实测发现 codehub-cli 合 MR 遇到权限不足时**会退出 0 但根本没合**,所以它复查一次 MR 状态,只有真变成 `merged` 才返回成功。

`github.rs:379-400` 的 `merge_pr` **没有这一步**,只看退出码(V3 那份**一字不差**,而且是 V3 的生产路径 —— 在真 GitHub 上跑了很久没听说假「完成」,所以这条的实际风险比纸面上低)。而合入成功会直接让活结清成「完成」(`ops.rs:240-248`)。**「完成」永远由人点**这条铁律没破(是人点的合入),但「合没合成」这个事实在 github 侧是听 `gh` 一面之词的。是不是要给 github 补上同样的复查,牵动设计,列入待拍板。

### 1.3 `Remote` 有几个方法从没被调过

**一个都没有。三个方法全都有 V4 的真实调用点**,行号见 §1.1 的表。所以「codehub 那半从来没被验证过」这个担心,**在这一层不成立**。

真正从来没被任何东西碰过的是别的东西 —— 底座里的死代码:

| 死掉的东西 | 位置 | 说明 |
|---|---|---|
| `CodehubRepoRef` 结构体 | `codehub.rs:30` | **全仓无人使用**。它原本是「新建 codehub 仓」那个函数的返回值,而那个函数在拷贝进 v4-engine 时没带过来,壳留下了 |
| 「新建仓 + 列仓」那段注释 | `codehub.rs:359-363` | 描述了一个 `project create --namespace-id ...` 函数,**这个函数不存在**。注释在说谎 |
| `github::remote_matches` | `github.rs:118` | 无人调用 |
| `RemoteReconcile` / `RemoteReconcileError` | `github.rs:131`、`:137` | **孤儿枚举** —— 全仓没有任何函数产出它们 |
| `github::list_open_issues` + `RemoteOpenIssue` + 解析函数 + 它的测试 | `github.rs:195-260`、`:610` | 无人调用 |
| `github::PROJECT_INIT_BRANCH` | `github.rs:271` | 无人调用 |

按仓规矩(「发现过时的实现路径,直接移除」),这些**该删**。本次只报不改。

另外一处不算死代码、但归错了地方的:`github::issue_branch`(`github.rs:262`)其实只是把号码拼成 `bw/issue-<n>`,**和平台毫无关系**,两个平台用的是同一个分支名。可它住在 `github` 模块里,文档还写着「for a given GitHub issue」,V4 有 7 处直接调它(如 `ops.rs:273`、`worktree.rs:75`)。行为没错,归属误导。

### 1.4 `provider` 字段:codehub 项目该写什么

一条链追下来,**是自洽的**:

| 环节 | 位置 | 值 |
|---|---|---|
| 接入屏平台选择器 | `onboard/mod.rs:618` | 两个 pill:`codehub` / `GitHub` |
| 选了 codehub 后写进命令 | `onboard/mod.rs:129-133` | `provider = "codehub"` |
| 区(host)从哪来 | `onboard/mod.rs:36-38`、`:61` | `green` / `open` / `yellow` 三选一,**默认 `open`(内源区)** |
| 存进库 | `bw-v4/src/store/schema.sql:25` | `provider TEXT NOT NULL DEFAULT ''`,注释:`'github' | 'codehub'`;空 = 未挂远端 |
| 认回来 | `remote.rs:60-70` | `"github"` 或**空串**→ GitHub;`"codehub"` → CodeHub;别的 → 报错 |

**所以 codehub 项目要写的就是 `provider = "codehub"`,`host` 写区的别名(`green`/`open`/`yellow`),不是域名。**

空值当 github 这条兼容,`remote.rs:58` 明确是为存量行留的。`trend.rs:152-157` 特意注明它也走同一份判断,免得两处分叉 —— 这处纪律守得不错。

**但有一处注释是错的**:`remote.rs:40` 把 codehub 的 `host` 描述成「green/yellow/inner-source domain」(域名)。它不是域名,是区的别名。`git.rs:708-710` 的注释才是对的,原话:codehub 的 host 存的是区的别名,不是域名,拼不出能点的地址。一个字段两处注释打架,而且错的那处正好在最该权威的分发工厂上。小,好改。

### 1.5 codehub 这块的总账

| 能力 | 状态 | 依据 |
|---|---|---|
| 认出 codehub 项目(provider 分发) | **已实现,未验证** | `remote.rs:60-70`;链路自洽见 §1.4 |
| 列仓 / 探仓 / 读远端名片 | **已实现,未验证** | `bridge/mod.rs:283`、`:339`、`:323` |
| clone(走 SSH) | **已实现,未验证** | `codehub.rs:307-371` |
| 开 MR | **已实现,未验证** | `codehub.rs:100-181` |
| 查开着的 MR | **已实现,未验证** | `codehub.rs:183-227` |
| 合 MR(带合入后复查) | **已实现,未验证** | `codehub.rs:229-291` |
| MR / issue 链接能点开 | **已实现,但代码级判定为坏** | §1.2 ①,`git.rs:746-767` |
| 环境条探测 codehub-cli | **已实现,代码级存疑** | §1.2 ②,`vm_build.rs:398` |
| 走势图「每周合入的 MR」 | **没实现**(如实留空) | `trend.rs:158-169`、`:83-85` |
| 新建 codehub 仓 | **没实现**(github 侧同样没有,对称) | `codehub.rs` 无此函数 |

**「未验证」在这里是字面意思:本机没有 codehub-cli,上面每一条都没有跑过一次。**

---

## 2 · Windows 这一块

### 2.1 今天到底编不编得过:**编不过**

照抄命令跑的结果:

```
cargo check --workspace --exclude app-desktop --target x86_64-pc-windows-gnu
→ error: failed to run custom build command for `libsqlite3-sys v0.30.1`
  error occurred in cc-rs: failed to find tool "x86_64-w64-mingw32-gcc"
```

**卡的原因是本机缺件,不是 BW 的代码有问题**:`libsqlite3-sys` 自带一份 `sqlite3.c` 要现编,需要 Windows 的 C 编译器(mingw-w64),这台 mac 上没装(`which x86_64-w64-mingw32-gcc` 找不到)。链路是 `bw-v4` → `sqlx` → `libsqlite3-sys`。

**但「不是代码问题」不等于「代码没问题」**:构建脚本在任何一行 BW 代码被检查之前就炸了。分开跑才看得出各自的状态:

| 目标 | 结果 | 依据 |
|---|---|---|
| `cargo check -p v4-engine --target x86_64-pc-windows-gnu` | **✅ 通过(7.84 秒)** | 连 `conpty-oxide`(内嵌终端的 Windows 后端)和 `windows-spawn` 一起编出来了 |
| `bw-v4` | **无证据** | 被 `libsqlite3-sys` 挡在门外 |
| `app-shell` | **无证据** | 同上(依赖 `bw-v4`) |

好消息:**Windows 专属代码全部住在 `v4-engine` 里**,而它自己是干净的。坏消息:壳和内核那两个 crate 今天一个字都没验到。

再强调一遍证明力的边界:**`cargo check` 不做链接**。就算把 mingw 装上、三个 crate 全绿,也只说明「类型和 cfg 分支自洽」,链不链得出 exe、跑起来对不对,一概不证明。

### 2.2 所有 Windows 分支,逐个过

代码里一共 7 处平台分叉涉及 Windows。逐个回答:怎么走 / 验没验过 / 最可能在哪炸。

| # | 位置 | 在 Windows 上怎么走 | 验过没 | 最可能在哪炸 |
|---|---|---|---|---|
| 1 | `v4-engine/src/win_cmd.rs:12-62` | 所有探测 / git / codehub-cli 的子进程都加 `CREATE_NO_WINDOW`(不闪黑窗);`.cmd`/`.bat` 不是可执行映像,改用 `cmd.exe /c` 托起 | 只交叉编译过(§2.1),**未真机** | 设计上是对的。风险在于**它被用在了不该用的地方**,见第 3 行 |
| 2 | `win_cmd.rs:17-22` `is_windows_script` | 按扩展名认 `.cmd`/`.bat` | 有内联单测(`win_cmd.rs:65-78`),CI 在跑 | 低风险 |
| 3 | `v4-engine/src/interactive_cli.rs:472-487` `run_skill` 的 Windows 分支 | **自相矛盾**,见下方展开 | **未真机**;V3 那份一字不差,但 V3 那边同样只在没开内嵌终端时才走,大概率也没被跑过 | **高**。交互式 claude 很可能拿不到终端,当场失败或静默挂住 |
| 4 | `v4-engine/src/pty_backend.rs:100` 起 conpty-oxide 后端 | 内嵌终端(▶跑 的主路径)。起 ConPTY 会话、读写两条阻塞线程、`.cmd` 再包一层 `cmd.exe /c`、被中止时靠 Job 连坐杀进程树 | **和 V3 那份一字不差**(`diff` 无输出),而它是 V3 的生产路径 —— **V3 若在 Windows 上用过,这条就有真实里程**(§0.2 问题 1) | 中高。ConPTY 那套句柄/EOF 逻辑真机才见真章;而且**验证工具本身在 Windows 上跑不了**,见第 6 行 |
| 5 | `app-shell/src/main.rs:41` 关掉 wry 的拖放处理器 | `with_disable_drag_drop_handler(true)`,不关的话计划屏六列拖不动 | **未真机** | 中。V4A-12 记的就是它,描述准确(§2.3) |
| 6 | `v4-engine/examples/pty_smoke.rs:99-100` | **硬编码起 `bash`** | — | **中**。Windows 默认没有 `bash`,这个唯一的 PTY 读回工具在目标平台上开箱跑不了 |
| 7 | `app-shell/src/chrome/mod.rs:325-327` 打开浏览器 | `cmd /C start "" <url>`,**没走 `win_cmd` 那层** | **未真机** | 低。会闪一下黑窗;链接已被限定必须 `https://` 开头(`chrome/mod.rs:319`),注入风险已挡 |

**第 3 行展开 —— 这条是自己跟自己打架:**

`interactive_cli.rs:468-471` 的注释原话是:在 Windows 上从 GUI 程序起一个控制台进程会**新开一个控制台窗口(用户能看见 agent)**。可紧接着 `:474` 用的是 `crate::win_cmd::tokio_cmd(binary)` —— 而这个辅助函数**专门就是来把窗口按掉的**(`win_cmd.rs:13` 那个 `CREATE_NO_WINDOW`)。

于是实际会发生的是:窗口不会出现;stdio 继承自一个没有控制台的 GUI 父进程;交互式 `claude` 拿不到任何终端。然后 `:484` 起进程、`:487` 等它退出。**最可能的结局是当场报错,或者静默挂到超时。**

这条路 V4 真的会走 —— `bw-v4/src/app/issue.rs:175` 在项目有真实工作区、但没开内嵌终端(指挥器、headless)时调的就是它。

### 2.3 两条已登记欠账,今天还准不准

**减负-14(内嵌终端的 Windows 后端只交叉编译核对过、没真机跑过)** —— **措辞准,但可能已经过时了。**

先说新证据:V4 那份 `pty_backend.rs` 和 V3 那份 **`diff` 无输出、一字不差**,而它是 V3 内嵌终端的生产路径。**只要 V3 在 Windows 上被用过并开过内嵌终端,这条欠账就基本可以结** —— 那 532 行有真实里程,只是当初没人把它记下来。这个我答不了,列在 §0.2 问题 1。

下面这条覆盖面的问题仍然成立:

它记的是 V3 那份 `bw-engine/src/pty_backend.rs`。V4 有**自己的一份** `v4-engine/src/pty_backend.rs`(532 行,2026-08-21 拷贝接管)。两份都没真机跑过,但严格说这条欠账不覆盖 V4 那份。

顺带查出一处**证据标注不实**:`v4-engine/src/pty_backend.rs:104-105` 的模块注释写着「只经 `cargo check --target x86_64-pc-windows-gnu -p bw-engine` 交叉编译检查」。这是 `v4-engine` 的文件,拷贝时没改 crate 名 —— **它引用的是另一个 crate 的验证证据**。本次核查实际验的是 `-p v4-engine`(§2.1 通过),该把注释改成实况。

**V4A-12(Windows 安装包没打;拖拽要关 wry 拖放处理器,写了但没真机验证)** —— **准。**

`app-shell/src/main.rs:39-41` 确实写了 `#[cfg(windows)] let cfg = cfg.with_disable_drag_drop_handler(true);`,注释也说明了原因(不关的话 WebView2 会屏蔽页面内拖放)。未真机验证这一点如实。Inno 安装脚本不在这个仓里这一点本次没有推翻。

### 2.4 路径与环境:逐处过

| 处 | 位置 | Windows 上怎样 | 判定 |
|---|---|---|---|
| 库文件落点 | `app-shell/src/bridge/mod.rs:174-197` | `%APPDATA%\BuildersWorkbench\workbench-v4.db`,`BW_DB` 可覆盖 | **没问题**。拼接混用了 `\` 和 `/`,但 Windows 的文件 API 和 SQLite 都吃混合分隔符 |
| 家目录 | `bridge/mod.rs:220-226` | `HOME` 取不到就退 `USERPROFILE`,再不行退 `.` | **没问题**,考虑到 Windows 了 |
| 工作区根 | `bridge/mod.rs:200-204` | `BW_WORKSPACES`,否则 `<家目录>\.builders-workbench\workspaces` | **没问题**(用 `PathBuf::join`,分隔符由标准库处理) |
| 资产目录 | `bridge/mod.rs:209-218` | 跟着库文件走;库路径是裸文件名时退回家目录 | **没问题**,而且这个边界情况已经想过了 |
| claude 二进制解析 | `v4-engine/src/claude_bin.rs:31-47` | 候选顺序:显式指定 → `BW_CLAUDE_BIN` → `%APPDATA%\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe` → `%APPDATA%\npm\claude.cmd` → PATH 里的 `claude` | **没问题**。`.cmd` 起不起得来由 `win_cmd` / ConPTY 各自包 `cmd.exe /c` 兜住 |
| `BW_SH_BIN` | — | **V4 里根本没有这个变量**。全仓只有 `bw-app`(V3)在用(`bw-app/src/lib.rs:1300`) | **问题不成立**。V4 不起 shell |
| worktree 路径拼接 | `bw-v4/src/git.rs`、`v4-engine/src/workspace.rs` | 一路 `PathBuf::join`,没有硬编 `/` | **没问题** |
| **PATH 里找程序** | `v4-engine/src/claude_bin.rs:15-22` | **有问题,见下** | **高风险**(V3 有同样的代码,但从来不拿它找 `gh`/`codehub`,所以 V3 的实战证明不了它) |

**`which_on_path` 在 Windows 上会骗人:**

```rust
pub fn which_on_path(exe: &str) -> Option<String> {
    std::env::split_paths(&path).map(|dir| dir.join(exe)).find(|p| p.is_file())
}
```

它**只按裸名字找,不试 Windows 的扩展名**。Windows 上这些程序落地叫 `gh.exe`、`cursor-agent.exe`、`codehub-cli.exe`,`dir.join("gh")` 永远找不到。

后果落在项目墙那条环境条上(`vm_build.rs:397-399`):**cursor-agent、codehub、gh 三项在 Windows 上恒报红,哪怕装得好好的**。claude 那一项侥幸没事 —— 它走的是 `claude_binary_candidates`,里头那两条 `%APPDATA%\npm` 路径把标准安装兜住了。

**为什么 V3 用了这么久没暴露**:V3 里 `which_on_path` **只被拿来找 `claude`,而且是候选链的最后一条兜底**(`bw-engine/src/claude_bin.rs:44`)—— Windows 上前面那两条 `%APPDATA%\npm` 路径早就命中了,根本轮不到它。V4 的环境条是**第一个**拿它去找 `gh` / `cursor-agent` / `codehub` 的地方。**所以 V3 的实战不能给这条背书,它是 V4 新开的暴露面。**

这正是 `claude_bin.rs:25-30` 那段注释在 2026-08-20 修过的**同一类 bug 的镜像**。那次是 macOS 被 Windows 专属路径坑了,注释末尾那句话现在反过来打在自己身上:**假的红灯比没有灯更坏。**

### 2.5 有没有 macOS-only 的假设混进 V4

**运行时代码里没有。**逐条查证:

- `interactive_cli.rs` 里的 `osascript`(`:501`)在 `#[cfg(target_os = "macos")]` 里面,Windows 编不进去。而且这条分支很诚实:`:508-515` 明确说拿不到 claude 的句柄、所以**如实报「未完成」**,不假装成功。
- `chrome/mod.rs:321-327` 打开浏览器三个平台都写了。
- `github.rs:211`、`:426` 里的 `"open"` 是 `gh` 的命令行参数(issue/PR 的 open 状态),不是 macOS 的 `open` 命令。同理 `onboard/mod.rs:38` 的 `"open"` 是 codehub 内源区的别名。**都不是平台假设。**

**脚本里有,但都不在产品路径上**,不影响 Windows 用户:`scripts/bundle-desktop.sh`、`scripts/point-bwdev-here.sh`、`scripts/verify-codegraph.sh` 依赖 macOS 的 `.app` 打包那一套。三个结构守卫(`guard-kernel-ui-free.sh` / `guard-no-cross-screen-import.sh` / `guard-screen-hooks.sh` / `guard-file-lines.sh`)是纯 grep,CI 在 ubuntu 上照跑,不含平台假设。

唯一真碍事的是 §2.2 第 6 行那条:`pty_smoke` 硬编 `bash`。它是**唯一**能在不碰 claude、不碰网关的前提下验证 PTY 后端的工具,偏偏在最需要验证的平台上开箱跑不了。

### 2.6 Windows 这块的总账

| 能力 | 状态 | 依据 |
|---|---|---|
| `v4-engine` 能为 Windows 编译 | **已实现并验证** | `cargo check -p v4-engine --target x86_64-pc-windows-gnu` 通过 |
| `bw-v4` / `app-shell` 能为 Windows 编译 | **无证据** | 被 `libsqlite3-sys` 缺 mingw-w64 挡住(§2.1) |
| 子进程不闪黑窗、`.cmd` 能起 | **已实现,未验证** | `win_cmd.rs:24-62`,有内联单测但只覆盖扩展名判断 |
| 内嵌终端(ConPTY) | **已实现,未验证** | `pty_backend.rs:100` 起;验证工具在 Windows 上跑不了 |
| 无内嵌终端时起 claude | **已实现,代码级判定有矛盾** | §2.2 第 3 行 |
| 关掉 wry 拖放处理器 | **已实现,未验证** | `main.rs:41` |
| 库/工作区/资产的路径落点 | **已实现,未验证**(代码级看没问题) | §2.4 前四行 |
| claude 二进制解析(含 `claude.cmd`) | **已实现,未验证**(代码级看没问题) | `claude_bin.rs:31-47` |
| 环境条探测 gh / cursor-agent / codehub | **已实现,代码级判定为坏** | §2.4 末,`claude_bin.rs:15-22` |
| Windows 安装包 | **没实现** | V4A-12,Inno 脚本不在本仓 |

---

## 3 · 拿到真环境后按这个顺序验

**这一节是这份报告最该用起来的部分。**每条写清楚:跑什么、期望看到什么、看到什么算失败。顺序是有讲究的 —— 前面的不过,后面的白验。

### 3.1 codehub(需要:一台装了 codehub-cli 并已 `auth login` 的机器)

| # | 跑什么 | 期望看到 | 看到什么算失败 |
|---|---|---|---|
| 1 | `which codehub` 和 `which codehub-cli` | **两个都有路径** | 只有 `codehub-cli` 有 → §1.2 ② 坐实,环境条恒红,得改 `vm_build.rs:398` |
| 2 | `codehub-cli project list --mine --limit 5 --json path_with_namespace,visibility,default_branch,last_activity_at,description` | 一个 JSON 数组,五个字段都在 | 字段名对不上 → `codehub.rs:467-476` 的解析结构要改 |
| 3 | 记下第 2 步里某个仓的 `default_branch` | 是 `master` 还是 `main` | 是 `master` → §1.2 ④ 坐实,手打地址那条路会卡 |
| 4 | `codehub-cli project view -p <仓> -H open --template '{{.ssh_url_to_repo}}'` | 一个 `ssh://...` 裸串 | 输出 `<no value>` 或空 → clone 会被 `codehub.rs:335-339` 挡下 |
| 5 | **把这个仓的网页地址,和 V3 那张映射表对一对**:`open` 该是 `open.codehub.huawei.com`、`green` 是 `codehub-g.huawei.com`、`yellow` 是 `codehub-y.huawei.com`(`bw-core/src/model.rs:1388`) | 映射表对得上 | 对不上 → 那张表本身过时了,修之前先更新它。**注意:这一步不再是「去发现规则」,规则已经在仓里,这一步只是复核** |
| 6 | 在 buddy 里接入一个 codehub 仓(接入屏 → 选 codehub → 选区 → **从列表里点**一个仓) | 名片能预填或如实说「没接管过」;clone 成功 | 报「没查成」→ 回到第 3 步看默认分支 |
| 7 | `sqlite3 <库> "SELECT provider, remote_host, remote_path FROM project WHERE slug='<slug>';"` | `codehub | open | <org>/<仓>` | `provider` 是空的 → 接入屏没写进去,查 `onboard/mod.rs:129-133` |
| 8 | **换一条路再来一次:手打仓地址,不从列表点** | 同第 6 步 | 报「没查成」而列表那条路好使 → §1.2 ④ 坐实 |
| 9 | 在这个项目上建一件活、▶跑、让它推出分支 | `bw/issue-<号>` 出现在远端 | — |
| 10 | 开 MR | 界面回一个 MR 号 | 报 branch not found → §1.2 ③ 的 `master` 兜底猜错了 |
| 11 | **在计划屏详情里点那个 MR 链接** | 浏览器打开真的 MR 页面 | **打不开 → §1.2 ① 坐实**(预期就是打不开) |
| 12 | 点「合入并完成」 | MR 真合了,活变「完成」 | 界面说成了但 MR 还开着 → `codehub.rs:252-291` 那段复查没起作用,这是最严重的一类失败,立刻停下 |
| 13 | `sqlite3 <库> "SELECT number,status,pr_number,settled_at FROM issue WHERE id='<id>';"` | 状态是完成、`pr_number` 是真实 iid、`settled_at` 有值 | `pr_number` 是 0 → MR 号没记回库 |
| 14 | 打开总览屏看走势图 | 「每周合入」那条**留空**,并有一句「远端不是 GitHub…」 | 画成了 0 → 那是假数据,比留空更坏(预期是留空,`trend.rs:81-84`) |

### 3.2 Windows(需要:一台内部 Windows 机器)

| # | 跑什么 | 期望看到 | 看到什么算失败 |
|---|---|---|---|
| 1 | 先在 mac 上 `brew install mingw-w64`,再跑 `cargo check --workspace --exclude app-desktop --target x86_64-pc-windows-gnu` | 全绿 | 有编译错 → 那才是 `bw-v4`/`app-shell` 真正的 Windows 问题,**这一步没过就别上真机** |
| 2 | 在 Windows 上 `cargo build -p app-shell --target x86_64-pc-windows-msvc` | 链接得出 exe | 链接错 → `check` 证明不了的那部分,现在见真章 |
| 3 | 改一行让 `pty_smoke` 在 Windows 上起 `cmd /C echo pty-ok` 而不是 `bash -c`,然后 `cargo run -p v4-engine --example pty_smoke` | 读回 `pty-ok` | 读不回 → ConPTY 后端根本不通,后面全免谈 |
| 4 | `cargo run -p v4-engine --example pty_smoke -- --teardown` | 5 秒内返回,孙进程被连坐杀掉 | 留下孤儿进程 → Job 的 kill-on-close 没生效 |
| 5 | `cargo run -p v4-engine --example pty_smoke -- --abort` | 中止后子进程照样被收尾 | 留下孤儿 → 同上 |
| 6 | 起壳,看项目墙那条环境条 | 装了的工具**显示绿** | **gh / cursor-agent / codehub 报红但其实装了 → §2.4 坐实**(预期就是报红) |
| 7 | `where gh` / `where codehub-cli` | 有路径 | 配合第 6 步用:命令行找得到、界面说找不到,就是 `which_on_path` 的锅 |
| 8 | 建一件活,▶跑,**用内嵌终端** | 终端里出现 claude 的 TUI,键盘能打字,输出在滚 | 黑屏/挂住 → ConPTY 的读写线程有问题;字符是乱码 → 编码问题(本次核查没覆盖这一项) |
| 9 | 在内嵌终端里粘一大段文字(几 KB) | 界面不卡 | 整个界面冻住 → `pty_backend.rs` 模块注释里担心的那个阻塞写真的发生了 |
| 10 | 拖动计划屏那六列 | 拖得动 | 拖不动 → `with_disable_drag_drop_handler(true)` 没起作用,V4A-12 |
| 11 | 找一件有真实工作区的活,走**不开内嵌终端**那条路(指挥器/headless) | 能起来 | 报错或挂住 → **§2.2 第 3 行坐实**(预期就是这个结果) |
| 12 | `sqlite3 %APPDATA%\BuildersWorkbench\workbench-v4.db "PRAGMA table_info(issue);"` | 表结构正常 | 库文件不在那 → `db_path()` 落点不对 |
| 13 | 点一个 MR 链接 | 浏览器打开,**不闪黑窗** | 闪一下 cmd 窗口 → §2.2 第 7 行,小事 |

---

## 4 · 查出来的真问题

按你的规矩分两类。**本次核查一行代码都没改** —— 交给下一个窗口处理。

### 4.1 能当场修的(小、明确、不牵动设计)

| # | 问题 | 位置 | 怎么修 |
|---|---|---|---|
| **修-0** | **codehub 的 MR / issue 链接从 SSH 地址推,必然点不开** —— 而 V3 早有一张跑通的区别名 → 域名映射表,V4 拷贝时弄丢了 | 界面按 provider 拼(照 `app-desktop/src/screens/op/issues.rs:25-38`);映射表照抄 `bw-core/src/model.rs:1388` | **V4 不许依赖 `bw-core`**,所以是在 V4 里**自带一份同样的八行映射**,不是 `use bw_core::…`。改界面那一侧即可,**未必要动 `git.rs`**。初稿把这条列成「要拍板」,是因为我当时不知道映射表已经存在 —— **现在它是能当场修的第一条** |
| 修-1 | `which_on_path` 不试 Windows 扩展名,导致三项探活在 Windows 上恒假红 | `v4-engine/src/claude_bin.rs:15-22` | 按 `PATHEXT`(或直接试 `.exe`/`.cmd`/`.bat`)逐个试一遍。**优先级最高**:界面骗人 |
| 修-2 | 探测找 `codehub`,实际调 `codehub-cli` | `app-shell/src/bridge/vm_build.rs:398` | 改成找 `codehub-cli`。**先做 §3.1 第 1 条确认**,别盲改 |
| 修-3 | `run_skill` 的 Windows 分支想要控制台窗口,却用了专门隐藏窗口的辅助函数 | `v4-engine/src/interactive_cli.rs:472-487` | 这条路不该走 `win_cmd::tokio_cmd`。要么直接用 `tokio::process::Command` 让控制台出来,要么改注释承认它就是没窗口 —— 但那样交互式 claude 拿不到 TTY,等于这条路在 Windows 上不成立,应当如实报错 |
| 修-4 | `pty_smoke` 硬编 `bash`,Windows 上开箱跑不了 | `v4-engine/examples/pty_smoke.rs:96-100` | 按平台选 `bash -c` / `cmd /C`。五行的事,却是 Windows 上唯一的 PTY 读回手段 |
| 修-5 | 一堆死代码 | `codehub.rs:30`;`github.rs:118`、`:131`、`:137`、`:195-260`、`:271`、`:610` | 按仓规矩直接删。`RemoteReconcile` / `RemoteReconcileError` 是**孤儿枚举**,全仓没有函数产出它们 |
| 修-6 | 注释说谎:描述了一个不存在的「新建仓」函数 | `codehub.rs:359-363` | 删掉或改成实况 |
| 修-7 | 注释说谎:把 codehub 的 `host` 说成域名,实际是区的别名 | `v4-engine/src/remote.rs:40` | 照 `git.rs:708-710` 那句改。错在最权威的分发工厂上,更该改 |
| 修-8 | 证据标注不实:V4 那份 PTY 后端的注释引用的是另一个 crate 的交叉编译证据 | `v4-engine/src/pty_backend.rs:104-105` | 改成 `-p v4-engine`(本次已验,§2.1) |
| 修-9 | `open_in_browser` 的 Windows 分支会闪黑窗 | `app-shell/src/chrome/mod.rs:325-327` | 走 `win_cmd::std_cmd`,或用 `ShellExecuteW` |

**注意**:修-1、修-3、修-4 都在 `v4-engine`;修-2、修-9 在 `app-shell`。都不在你划的禁区里(总览屏/计划屏/会话屏/`trend.rs`/`health.rs`/`git.rs`),可以直接动。

### 4.2 要拍板的(牵动设计,先别动)

| # | 事情 | 为什么要你定 |
|---|---|---|
| ~~拍-1~~ | ~~codehub 的 MR / issue 链接怎么修~~ | **撤销** —— 比过 V3 之后不用拍板了:V3 已有跑通的做法和映射表,照抄即可。**改列为 §4.1 修-0** |
| 拍-2 | **接入阶段那 4 条路要不要收进 `Remote` 分发**(§1.1) | 探仓 / 列仓 / 读名片 / clone 全在分发体系之外,每个调用点各写各的 `if`。今天没错,但「加平台只改一处」这条承诺对它们不成立。收进去是重构,牵动 `bw-v4` 和 `app-shell` 两侧 |
| 拍-3 | **github 的合入要不要补一次合入后复查**(§1.2 ⑤) | codehub 那侧因为踩过坑做了复查,github 没有。合入成功会直接结清成「完成」。要不要对齐,是产品可靠性的取舍 |
| 拍-4 | **`issue_branch` 要不要挪出 `github` 模块**(§1.3 末) | 纯字符串拼接,和平台无关,两个平台共用,却住在 `github` 里、文档说是给 GitHub 用的。V4 有 7 处直接调它,挪窝是纯改名,但会碰到不少文件 |
| 拍-5 | **手打仓地址时的默认分支怎么定**(§1.2 ④) | 现在写死 `main`。可以先问一次远端的默认分支再查,多一次往返;也可以 `main` 失败后自动试 `master`;也可以就让它失败、让人自己填分支。三条路体验不同 |

---

## 5 · 建议登记的新欠账(可直接粘贴)

**没有直接改 `docs/LEFTOVERS.md` 和 `docs/code-schemes.md`** —— 那两份是另一个窗口大概率也在动的文件,避免撞车。下面是照现有体例写好的条目,谁处理谁贴。

### 5.1 先在 `docs/code-schemes.md` 登记新系列

按现有登记表核对过:字母 A/B/C/D/G/K/L/M/P/R/S/T/V/W 都已被占用。沿用 `减负-N`、`试点-N` 那个中文前缀的先例,开 **`兼容-N`**,不与任何字母系列撞车。加一行:

```
| **兼容-1 … 兼容-N** | `docs/LEFTOVERS.md`「当前开着」表 | 2026-08-21「V4 对 codehub 与内部 Windows 的兼容性核查」查出的欠账(`docs/v4-prototype/codehub-windows-readiness.md`):远端链接、探测名、平台分支、真环境验证清单。**中文前缀,不与任何字母系列撞车**,同 `减负-N` / `试点-N` 的做法 | 开着 |
```

### 5.2 再往 `docs/LEFTOVERS.md`「当前开着」表加这几行

```
| **兼容-1** | codehub 项目的 MR / issue 链接**必然点不开**:`browse_base` 从 SSH origin 推地址,带 `:2222` 端口且 SSH 主机名 ≠ 网页主机名。**根因是 V4 拷贝时弄丢了 V3 那张区别名 → 域名映射表** | **可当场修**(不需要真环境) | 坏的:`git.rs:746-767`;V3 跑通的写法:`app-desktop/src/screens/op/issues.rs:25-38` + `bw-core/src/model.rs:1388`;V4 要自带一份映射(不许依赖 `bw-core`) |
| **兼容-2** | 环境条探测找的是 `codehub`,实际起的进程是 `codehub-cli`(8 处全是),可能导致装了也报红 | 有 codehub-cli 的机器上先确认 | `vm_build.rs:398` vs `codehub.rs:65/154/188`;验证走 §3.1 第 1 条 |
| **兼容-3** | `which_on_path` 只按裸名字找,不试 Windows 扩展名 → Windows 上 gh / cursor-agent / codehub 三项探活恒假红 | **可当场修**(不需要 Windows 机器就能改对) | `claude_bin.rs:15-22`;同类 bug 的镜像见该文件 `:25-30` 注释(2026-08-20 修过 macOS 那一半) |
| **兼容-4** | `run_skill` 的 Windows 分支自相矛盾:注释要控制台窗口,代码用的却是专门隐藏窗口的 `win_cmd::tokio_cmd`;GUI 父进程无控制台 → 交互式 claude 拿不到 TTY | **可当场修**(要拍板的是改注释还是改行为) | `interactive_cli.rs:467-487`;V4 真会走到,见 `issue.rs:175` |
| **兼容-5** | `pty_smoke` 硬编 `bash`,Windows 上开箱跑不了 —— 而它是**唯一**不碰 claude、不碰网关就能验 PTY 后端的工具 | **可当场修**(五行) | `pty_smoke.rs:96-100`;减负-14 里那句「`bash -c` 换 `cmd /C echo pty-ok`」正是指这个 |
| **兼容-6** | `v4-engine` 里从 `bw-engine` 拷来的死代码没清:`CodehubRepoRef`、`remote_matches`、`RemoteReconcile`/`RemoteReconcileError`(孤儿枚举,无人产出)、`list_open_issues` 一套、`PROJECT_INIT_BRANCH`;外加两处说谎的注释(不存在的「新建仓」函数、把 codehub host 说成域名) | **可当场删** | `codehub.rs:30/246-250`;`github.rs:118/131/137/195-260/271/610`;`remote.rs:40` |
| **兼容-7** | `bw-v4` / `app-shell` 的 Windows 可编译性**今天无任何证据** —— 交叉编译被 `libsqlite3-sys` 缺 mingw-w64 挡在门外,只有 `v4-engine` 单独验过 | 装 mingw-w64 即可解(`brew install mingw-w64`) | 见该文 §2.1;验证走 §3.2 第 1 条 |
| **兼容-8** | **先确认再决定**:V4 那份 `pty_backend.rs` 与 V3 那份 **`diff` 无输出、一字不差**,而它是 V3 内嵌终端的生产路径 —— **V3 若在 Windows 上用过并开过内嵌终端,减负-14 基本可以结**,确认之前别当「没验过」重复排期。另有一处小的:模块注释引用的是另一个 crate 的验证证据 | **先问,再排** | `pty_backend.rs:104-105`;确认办法见报告 §0.2 问题 1 |
| **兼容-9** | 接入阶段的 4 条远端调用(探仓/列仓/读名片/clone)绕开 `Remote` 分发,各自手写 provider 分支 —— 「加平台只改一处」这条设计承诺对它们不成立 | 待拍板(见该文 §4.2 拍-2) | `bridge/mod.rs:283/323/339`、`project.rs:167-168` |
```

---

## 6 · 一句话总结

**codehub**:七条能力全接通了、链路自洽,但 **V4 这边一次都没在真环境跑过**;代码级就能判定坏掉的有一条(MR 链接 —— 而且比过 V3 才知道,**根因是拷贝时弄丢了一张现成的映射表,修法在仓里**),存疑的有一条(探测名),行为不对称的有三处 —— 其中两处比过 V3 后**降级**了(V3 拿同样的代码在生产上跑着)。

**Windows**:所有平台专属代码住的那个 crate 单独编得过,壳和内核**今天无证据**;有两处代码级就能判定会出事(探活假红、无内嵌终端时起 claude)—— 这两处**都是 V4 新开的暴露面**,V3 有同样的代码但从来走不到,所以 V3 的实战给不了它们背书。内嵌终端那份则相反:**和 V3 一字不差**,可能早有真实里程,等你确认(§0.2 问题 1)。

**这份报告没有实机验证过任何东西**,但它做了一件下一个窗口不必重做的事:**把每条发现拿去和真跑着的 V3 对了一遍**,分清了哪些是「V4 拷坏了」(修法现成)、哪些是「V4 新开的口子」(V3 证明不了)、哪些是「V3 扛过来了」(可以降级)。剩下三个问题我答不了,列在 §0.2,**你一句话就能定**。

真环境验证清单在 §3 那两张表 —— 每条写清楚跑什么、期望看到什么、看到什么算失败。
