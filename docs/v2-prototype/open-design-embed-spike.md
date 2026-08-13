# Open Design 内嵌穿刺（2026-08-13）

> **30 秒导读**：这是预研记录，不是功能规格。对着本机正在跑的 Open Design 0.16.1 实打实打过 HTTP / 命名管道。要回答两件事：buddy 进度屏能不能嵌它的界面；能不能把 buddy 的 skill / 提示词带进去。正式开发另开设计篇。

**结论**：界面可以嵌（HTTP 层已放行，buddy WebView 里还要再点一次确认）；skill / 提示词可以带进去，而且不必开一场真正的生成。

---

## 1. 本机事实

| 东西 | 值 |
|---|---|
| 软件 | `D:\software\Open Design\Open Design.exe` 0.16.1 |
| 网页服务 | 命名管道 `\\.\pipe\open-design-release-stable-win-web` → `http://127.0.0.1:51298` |
| daemon | 同前缀 `-daemon` → `http://127.0.0.1:59420` |
| 发现办法 | 管道发 `{ "type": "status" }`，回 `{ url, pid, state }`。端口每次启动会变，不要写死。 |

daemon 的 `/api/mcp/install-info` 里 `webBaseUrl` 是 `null`，不能当发现源。以管道 STATUS 为准。

---

## 2. 能不能嵌

打过：

- `GET http://127.0.0.1:51298/` 首页
- `GET .../projects`
- 带 `Origin: http://127.0.0.1:8080` + `Sec-Fetch-Dest: iframe` 的首页
- 一条真实会话页 `.../projects/<id>/conversations/<cid>`

以上全部 **200 HTML，没有 `X-Frame-Options`，没有 `Content-Security-Policy` / `frame-ancestors`**。页面源码里也没有 frame 策略 meta。

本地抛了 `docs/v2-prototype/_spike-open-design/iframe.html`（throwaway），用系统浏览器打开即可肉眼看 iframe。端口以管道 STATUS 为准，文件里写的是穿刺当时的 `51298`。

**还没在 buddy 的 wry WebView 里嵌过。** Dioxus 桌面壳自己是一块 WebView；从自定义协议页 iframe 到 `127.0.0.1` 可能碰到 WebView2 的本地网络限制。这是落地时第一张烟。daemon 报告 `desktopAuthGateActive: true`，但本次从本机 HTTP 调 API 都没有要鉴权。若嵌进去后 UI 出来、点了没反应，优先查这块。

无窗口拉起（只起 daemon+网页、不弹 Open Design 窗口）**这次没穿刺**。管道名是固定的，本机已有一份在跑，不能并行起第二份。

---

## 3. 能不能带 buddy 的 skill / 提示词

三条路，两条已经实打实走过，一条故意没走（会真烧生成）。

### 3.1 会话里塞提示词（已通，不生成）

`POST /api/projects/:id/conversations` 带 `seedMessages`。

已写入对话 `d7cde42f-9705-44ad-b439-54b5fbe53e05`（标题 `buddy-spike-do-not-keep`，项目 `1bd653a9-c2e4-4dec-b11f-3b2d1e6dcb24`）。再 `GET .../messages` 能读回 Buddy 约束原文。Open Design 里打开该项目应能看见这条用户消息。

### 3.2 把 buddy SKILL.md 装成 Open Design 插件（已通，不生成）

`POST /api/plugins/install` `{ "source": "./相对目录" }`，SSE 回 success。

- 装上了 `buddy-spike-prototype`（throwaway）。
- Open Design 把 buddy 风格的 `SKILL.md` 解析成插件，并授予 `prompt:inject`。
- `POST /api/plugins/buddy-spike-prototype/apply` 返回将注入的 query / pipeline / `capabilitiesRequired: ["prompt:inject"]`。
- **apply 不会改项目上的 `appliedPluginSnapshotId`**。真正钉到项目、开跑，要再 `POST /api/runs`。

Windows 注意：`D:\...` 绝对路径**不会**被当成本地源（检测只认 `/`、`./`、`~`），会 404 去市场找同名。穿刺时把插件拷到 Open Design 的 `runtime/` 下，用 `./buddy-spike-plugin` 才装上。落地要走 `./` 相对路径、或 `upload-folder` / zip。

### 3.3 开工时把提示词当 message（API 在，故意没跑）

`POST /api/runs` `{ projectId, message, pluginId?, skillId? }`。空 body 立刻失败，错误是 `message required`，没有子进程。带真实 message 会开 5–30 分钟生成，本次预研不烧。

源码写明纯网页模式跑插件会 409。以后注入必须 **daemon + 网页一起在**，不能只嵌静态页。

---

## 4. 落地时建议的顺序

1. buddy 用命名管道发现 web/daemon URL；没有服务就诚实空态。
2. 原型进度 iframe 打开 web URL 首页。先在 wry 里确认能看见、能点。
3. 注入：先 3.1（会话 seed）或 3.2（安装+用户在 Home 点插件）；`/api/runs` 留给「真的要 Open Design 开始画」再接。
4. 无窗口工作台、WebView 本地网络限制，是下一张穿刺，不是本记录已证事实。

---

## 5. 本机留下的 spike 痕迹（可删）

- Open Design 插件 `buddy-spike-prototype`
- 对话 `buddy-spike-do-not-keep`
- `docs/v2-prototype/_spike-open-design/`（iframe.html + 插件 SKILL.md）
- `%APPDATA%\Open Design\namespaces\release-stable-win\runtime\buddy-spike-plugin\`

卸载插件：`POST http://127.0.0.1:<daemon>/api/plugins/buddy-spike-prototype/uninstall`
