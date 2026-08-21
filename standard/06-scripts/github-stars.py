#!/usr/bin/env python3
"""GitHub star 数 —— buddy 铺的现成采集脚本(可回溯,retro = true)。

口径:到**每周周末那一刻**为止,这个仓被 star 的累计数。存量,不是流量。

**这份脚本存在的意义,一半是它本身,一半是它证明的那件事:**
star 数看起来只有「当前值」,像是典型的不可回溯指标。但 stargazers 接口带上
`Accept: application/vnd.github.star+json` 就会返回每个 star 的 `starred_at`,
于是任意一天的存量都能倒推出来 —— 它其实是可回溯的。

**所以判 retro = false 之前,先真的去找一遍有没有带时间戳的接口。** 判错的代价
很实:一条本来能画出四周走势的指标,会被画成只有一个孤点。

自己复算:

    gh api -H "Accept: application/vnd.github.star+json" \\
      "repos/<owner>/<repo>/stargazers?per_page=100&page=N" --jq '[.[].starred_at]'

铺一次就归这个项目,buddy 之后再也不覆盖它。
"""

import argparse
import datetime
import json
import subprocess
import sys

PER_PAGE = 100
MAX_PAGES = 100  # 一万个 star 封顶;真到了这个量级该换成增量存,不是硬撑


def weeks_in(since: datetime.date, until: datetime.date):
    out, cur = [], since - datetime.timedelta(days=since.weekday())
    while cur < until:
        y, w, _ = cur.isocalendar()
        out.append((f"{y}-W{w:02d}", cur))
        cur += datetime.timedelta(days=7)
    return out


def repo_slug() -> str:
    proc = subprocess.run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
        capture_output=True,
        text=True,
        check=True,
    )
    return proc.stdout.strip()


def fetch_starred_at(slug: str):
    """所有 star 的时刻。翻页拉全 —— 存量指标必须拿全量才算得出历史。"""
    out = []
    for page in range(1, MAX_PAGES + 1):
        proc = subprocess.run(
            ["gh", "api", "-H", "Accept: application/vnd.github.star+json",
             f"repos/{slug}/stargazers?per_page={PER_PAGE}&page={page}"],
            capture_output=True,
            text=True,
            check=True,
        )
        rows = json.loads(proc.stdout)
        if not rows:
            break
        for r in rows:
            raw = r.get("starred_at")
            if raw:
                out.append(raw)
        if len(rows) < PER_PAGE:
            break
    else:
        raise RuntimeError(f"star 超过 {MAX_PAGES * PER_PAGE} 个,这份脚本拉不全")
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
        stamps = []
        for raw in fetch_starred_at(repo_slug()):
            try:
                stamps.append(
                    datetime.datetime.fromisoformat(raw.replace("Z", "+00:00")).timestamp()
                )
            except ValueError:
                continue  # 解析不出来的跳过,不猜
    except FileNotFoundError:
        print("本机没有 gh(GitHub CLI),或者不在 PATH 里", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as e:
        print(f"gh 调用失败:{e.stderr.strip()}", file=sys.stderr)
        return 2
    except (json.JSONDecodeError, RuntimeError) as e:
        print(f"{e}", file=sys.stderr)
        return 2

    tz = datetime.datetime.now().astimezone().tzinfo
    since = datetime.date.fromisoformat(args.since)
    until = datetime.date.fromisoformat(args.until)

    points = []
    for label, monday in weeks_in(since, until):
        # 存量:到这一周末(= 下周一 00:00)之前 star 过的累计数。
        end = datetime.datetime.combine(
            monday + datetime.timedelta(days=7), datetime.time(0, 0), tz
        ).timestamp()
        points.append({"week": label, "value": sum(1 for t in stamps if t < end)})

    json.dump({"points": points}, sys.stdout, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
