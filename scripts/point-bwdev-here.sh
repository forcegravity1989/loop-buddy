#!/usr/bin/env bash
# scripts/point-bwdev-here.sh — 让 computer-use 免交互的自愈闭环:把这个
# worktree 的最新构建接到一个**长期稳定、只注册过一次**的 macOS app 上。
#
# 背景(2026-07-29/30 踩出来的两个真坑,都亲测过):
#
# 1. computer-use 的 request_access 认的"已安装应用"名单在同一次对话里现造
#    的新 app 认不出来(哪怕真的用 lsregister 注册进 LaunchServices、Spotlight
#    也查得到,返回 "doesn't match any installed or running application")。
#    因此不能每个 worktree 各建一个新 app——那样永远在追一个刚造出来的新
#    身份。做法:只造一个长期不变的壳 ~/Applications/BWDev.app(身份
#    dev.buildersworkbench.bwdev,注册一次后不再变),往它的
#    Contents/MacOS/bwdev-launcher 里塞真实二进制。
#
# 2. 这个可执行文件必须是**真实拷贝**进去的 Dioxus/wry 二进制本体,不能用
#    "exec 到符号链接"这种取巧写法——试过 shell 脚本 exec 一个符号链接
#    (指向 target/debug/builders-workbench),窗口能开、screenshot 能看到
#    真内容(compositor 级过滤只看 app 是否在授权名单,不管是不是 exec 链),
#    但**点击全部失败**(request_access 报的"程序坞"错误,即便刚
#    open_application 激活过)——推断是 exec() 把进程镜像换成了不在任何
#    .app bundle 里的裸二进制后,里面的 NSApplication 无法正确认领自己的
#    bundle 身份,于是从没能真正成为 AppKit 意义上的前台可交互窗口。换成
#    直接把二进制拷进 Contents/MacOS/(和已有的 bundle-desktop.sh 同一个
#    思路)后重新测试,点击依旧失败——这确认了点击受阻是这个 app 本身更深的
#    Dioxus/wry 窗口层级限制(此前 session 就记录过"clicks still blocked"),
#    不是打包方式的问题。**screenshot 两种打包方式下都真实可用**,这是今天
#    验证 Skill Hub 视觉一致性真正拿到像素证据的路径:
#      BW_SEL=skill:<uuid> 或 BW_HUB=<hub> 直接终端调用
#      Contents/MacOS/bwdev-launcher(不要用 `open -a`——env 变量传不进去,
#      同 bundle-desktop.sh 已有的注释),再用 computer-use 的 screenshot
#      (不是 click)读出真实渲染。
#
# 用法:cd 到目标 worktree,跑 ./scripts/point-bwdev-here.sh,然后:
#   BW_SEL=skill:<uuid> ~/Applications/BWDev.app/Contents/MacOS/bwdev-launcher &
# 配合 computer-use screenshot 使用。
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p app-desktop

APP=~/Applications/BWDev.app
cp target/debug/builders-workbench "$APP/Contents/MacOS/bwdev-launcher"
chmod +x "$APP/Contents/MacOS/bwdev-launcher"
codesign --sign - --force --deep "$APP" >/dev/null 2>&1

echo "[point-bwdev-here] BWDev.app 现在是这个 worktree 的最新构建:$(pwd)"
echo "[point-bwdev-here] 用法示例:BW_SEL=skill:<uuid> $APP/Contents/MacOS/bwdev-launcher &"
