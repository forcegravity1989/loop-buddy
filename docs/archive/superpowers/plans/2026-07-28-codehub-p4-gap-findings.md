# P4 GAP 诊断记录(2026-07-28,用户录 maas 后)

> ⚠️ **历史档案(2026-08-17 归档)**。一次性诊断记录,下一行自陈「已解决」。

> **状态(2026-07-29 更新):已解决。** 本件记录的 clone HTTPS 504 根因,由
> 后续 `P4-fix` commit 修掉——**走 SSH clone(本件「候选修复方向 A」)**:`codehub::clone_repo`
> 先 `project view --template .ssh_url_to_repo` 取 SSH URL(SSH host szv-open:2222
> ≠ API host open,不手拼),再 raw `git clone` + `GIT_SSH_COMMAND` accept-new。
> scratch + 用户真实录双验证:clone Ok、remote 设、CODEHUB connector mint、
> trio 同步 codehub(iid 16/17/18)。**方向 B/C 排除**(504 非代理配置/非 token)。
>
> 下面的「候选修复方向」+「auto-mint 行为 GAP」段是**修前快照**(历史)。其中
> auto-mint 那条:**决议 = 对齐 github、不搞特例**——github Existing clone 失败在
> `CompleteCreation` 也走 auto-mint(`lib.rs` 的 provision_workspace 对所有 workspace 空项目),
> codehub 同样即「对齐」,clone SSH 修好后 workspace 非空 auto-mint 不触发、假象消失;
> 真「clone 失败就停『未接上』」需持久化「尝试过远端」标志位,**转长期 TODO,非步1**。
>
> 另:claude CLI spawn「program not found」是**另一个独立 gap**(agent 步3),根因
> `BW_CLAUDE_BIN` 没设(Rust Windows 不做 PATHEXT),已配 env 解决,非本件 scope。

## 症状(用户 DB `AppData/Roaming/BuildersWorkbench/workbench.db` 读回为证)

maas-locate 项目行:
- `provider="codehub"` ✓
- `remote_host="github.com"` ✗(默认值,应为 `open.codehub.huawei.com`)
- `remote_path=""` ✗(空,应为 `innersource/AI-Coding_G/maas`)
- `workspace_path` 设了,但内容是 buddy 本地 mint(`PROJECT.md`/`README.md`/`.gitignore`),**不是 maas clone**(maas 该有 `governance/`/`docs/`/`.claude/` 等)
- connector 表只有 `git-repo`(workspace 路径),**没有 `codehub-repo` connector**
- issue 表空(没 trio)

## 根因(隔离诊断证实)

scratch example 直接 dispatch `CreateProject{provider:codehub, codehub:Some(CodehubOrigin{host,path})}`(绕开 UI)→ handler 的 codehub 分支**确实跑了**(事件 `克隆 codehub 仓 Started`),但 `bw_engine::codehub::clone_repo` 调 `codehub-cli repo clone <path> <dest> -H <host>` **失败**:

```
fatal: unable to access 'https://open.codehub.huawei.com/innersource/AI-Coding_G/maas.git/':
       CONNECT tunnel failed, response 504
error: git clone failed: exit status 128
```

**`git clone` 走 HTTPS 到 `open.codehub.huawei.com` 经代理隧道 504 失败。** 注意:codehub API 调用(`project view`/`issue list`/`mr list`,P3 验过)走得通(token 走 keyring、Go HTTP 客户端);但 `git clone` HTTPS 走 git 自己的 HTTP 栈 + 代理,504。两条路 HTTP 栈不同。

### 下游连锁(全由 clone 失败派生)

1. clone 失败 → codehub 分支的 **Err 臂**(只 emit `ConnectorSynced ok:false` + `ActionProgress Fail`,**不 set_remote、不 mint connector**)
2. project 行留下 INSERT 默认:`remote_host="github.com"`、`remote_path=""`、`workspace_path=""`、无 connector
3. 后续(创建流走到 CompleteCreation / 项目打开)`workspace_path 空 + workspaces_root Some` 触发**自动本地兜底**(`lib.rs:4015` 的 `provision_workspace`)→ 写 `PROJECT.md`/`README.md` + `set_workspace` + mint `git-repo` connector
4. trio(`seed_standard_issue_trio`)gate 在 `remote_path 非空`才建 —— 但 `remote_path=""`(clone 没设)→ **trio 短路,不建 3 卡** → issue 看板空

**所以 4 个症状(无 3 卡 / 看板空 / workspace 是本地 mint / 无 codehub connector)全源自一个根因:clone HTTPS 504。** handler 逻辑 + UI 都没问题(scratch 证实 codehub 分支会跑、set_remote 在 clone Ok 时会设)。

## 候选修复方向(未实施,留新窗口与用户定)

- **A. clone 改走 SSH URL**:交接 SSH 地址是 `ssh://git@szv-open.codehub.huawei.com:2222/innersource/AI-Coding_G/maas.git`。codehub-cli help 明说「SSH/SCP input is passed to git unchanged and never receives an HTTP token」——SSH clone 走用户 SSH key,不经 HTTPS 代理隧道。`codehub::clone_repo` 应按 host 映射 API-host→SSH-host(open.codehub.huawei.com → szv-open.codehub.huawei.com:2222)构 SSH URL 传 `codehub-cli repo clone <ssh_url> <dest>`。**前提**:用户本机 SSH key 对 szv-open 有权限(交接给了 SSH URL,但未实测 SSH clone 通不通)。
- **B. 配 git HTTPS 代理 / 直连**:504 是代理隧道,可能 git http.proxy 配错或该走直连。改 git config 或 env。
- **C. codehub-cli 是否真嵌了 token**:help 说 HTTPS input 会用 host 的 private token 重建 URL,但 504 是隧道层、不是 401,token 应已嵌(否则是 401 不是 504)。所以不是 token 问题,是代理/网络层。

**推荐 A**(SSH URL),但需先实测 `codehub-cli repo clone ssh://git@szv-open.codehub.huawei.com:2222/innersource/AI-Coding_G/maas.git <tmp>` 在用户机通不通(SSH key 权限)。若 SSH 也不通,回退 B(查 git 代理)。

## 附:clone 失败时的「裸项目 + 自动本地兜底」行为本身

clone 失败后项目变「裸(provider=codehub 但无 remote/workspace)+ 后续自动本地 mint」——这个**降级链本身可商榷**:codehub 接入失败应该如实停在「未接上」(像 github Existing 分支那样「不兜底本地 mint,拿无关空仓冒充已接入更不诚实」),而不是悄悄走本地兜底让用户以为接上了。**这是 P4 修 clone 之外的一个行为 GAP**(codehub 分支 Err 臂后的兜底链),留修时一并议。

## 环境备注

- scratch example 已删(不进仓)。诊断用 throwaway DB + temp ws。
- mingw(dlltool+as)在 `C:\Users\<你>\mingw64\bin`,新终端要重开 / `export PATH` 才见。
- codehub-cli v1.3.4、token keyring(green/open 两区),maas=内源 open。
