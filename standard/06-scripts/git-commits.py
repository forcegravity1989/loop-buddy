#!/usr/bin/env python3
"""每周提交数 —— buddy 铺的现成采集脚本(可回溯,retro = true)。

口径:**当前分支**上,提交时刻落在 `[周一 00:00, 下周一 00:00)`(本机时区)
的提交数。

**故意不带 `--all`。** 带上它会把 remote-tracking 分支和别的工作目录的提交都
算进来,于是一次 `git fetch` 就能把这个数字刷上去 —— 那就成了可以造假的数。

**故意不用 `git log --since/--until` 截窗口**,而是把提交时刻全取回来自己按
时间戳过滤。两个理由都是真踩出来的:

1. `--until=<下周一>` 会把下周一那一整天算进来(git 用 approxidate 解析这类
   不带时刻的日期),于是每一周都多算下一周的第一天。
2. `--since` 会提前停止遍历 —— 提交时刻不严格单调(rebase、cherry-pick、机器
   时钟)时会漏掉一批,同一条命令隔几分钟跑结果都可能不一样。

自己复算某一周(s / u 是那一周的起止 unix 秒):

    git log --pretty=format:%ct | awk -v s=<s> -v u=<u> '$1>=s && $1<u' | wc -l

铺一次就归这个项目,buddy 之后再也不覆盖它 —— 要改口径直接改这里。
"""

import argparse
import datetime
import json
import subprocess
import sys


def weeks_in(since: datetime.date, until: datetime.date):
    """窗口内的 ISO 周,旧的在前。until 是开区间右端。"""
    out, cur = [], since - datetime.timedelta(days=since.weekday())
    while cur < until:
        y, w, _ = cur.isocalendar()
        out.append((f"{y}-W{w:02d}", cur))
        cur += datetime.timedelta(days=7)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--since", required=True)
    ap.add_argument("--until", required=True)
    ap.add_argument("--granularity", default="week")
    args = ap.parse_args()

    if args.granularity != "week":
        print(f"只支持 --granularity week,收到 {args.granularity}", file=sys.stderr)
        return 2

    try:
        proc = subprocess.run(
            ["git", "log", "--pretty=format:%ct"],
            capture_output=True,
            text=True,
            check=True,
        )
    except FileNotFoundError:
        print("本机没有 git,或者不在 PATH 里", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as e:
        print(f"git log 失败:{e.stderr.strip()}", file=sys.stderr)
        return 2

    stamps = [int(x) for x in proc.stdout.split() if x.isdigit()]
    tz = datetime.datetime.now().astimezone().tzinfo
    since = datetime.date.fromisoformat(args.since)
    until = datetime.date.fromisoformat(args.until)

    points = []
    for label, monday in weeks_in(since, until):
        lo = datetime.datetime.combine(monday, datetime.time(0, 0), tz).timestamp()
        hi = lo + 7 * 24 * 3600
        points.append({"week": label, "value": sum(1 for t in stamps if lo <= t < hi)})

    json.dump({"points": points}, sys.stdout, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
