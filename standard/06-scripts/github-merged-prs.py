#!/usr/bin/env python3
"""每周合入的 PR 数 —— buddy 铺的现成采集脚本(可回溯,retro = true)。

口径:**远端**上 merged 时刻落在这一周的 PR 数。

**本机的合并提交数不是这个口径,不要混用。** squash 合的 PR 在本机一个合并提交
都没有,而本机 `git pull` 产生的合并提交又不对应任何 PR。合没合入看远端,那个
造不了假。

**一趟拉全,本地按周分桶**,不按周去发查询:按周查画四周就发四次,而且搜索接口
本身不稳(真机上撞到过 `api.github.com/search/issues` 直接断连)。

自己复算:

    gh pr list --state merged --json mergedAt --limit 1000

铺一次就归这个项目,buddy 之后再也不覆盖它。
"""

import argparse
import datetime
import json
import subprocess
import sys

CAP = 1000


def weeks_in(since: datetime.date, until: datetime.date):
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
            ["gh", "pr", "list", "--state", "merged", "--json", "mergedAt",
             "--limit", str(CAP)],
            capture_output=True,
            text=True,
            check=True,
        )
    except FileNotFoundError:
        print("本机没有 gh(GitHub CLI),或者不在 PATH 里", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as e:
        print(f"gh pr list 失败:{e.stderr.strip()}", file=sys.stderr)
        return 2

    try:
        rows = json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        print(f"gh 的输出不是 JSON:{e}", file=sys.stderr)
        return 2

    # 顶到上限 = 可能被截断了。宁可整条线不画,也不端一个少算的数出去。
    if len(rows) >= CAP:
        print(f"已合并 PR 超过 {CAP} 条,一趟拉不全 —— 这次不给数,免得少算",
              file=sys.stderr)
        return 2

    tz = datetime.datetime.now().astimezone().tzinfo
    stamps = []
    for r in rows:
        raw = r.get("mergedAt")
        if not raw:
            continue
        # `2026-08-17T01:02:03Z` → 带时区的时刻。解析不出来的跳过,不猜。
        try:
            stamps.append(
                datetime.datetime.fromisoformat(raw.replace("Z", "+00:00")).timestamp()
            )
        except ValueError:
            continue

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
