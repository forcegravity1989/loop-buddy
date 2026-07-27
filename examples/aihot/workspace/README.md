# aihot 日报

AI 热点圈信息过多——按用户自己的关注面(关键词配置),每天从多个真实来源(Hacker News、arXiv)聚合、去重、过滤,生成一份可读的每日 AI 热点摘要。

## 快速开始

```bash
python3 -m aihot.main            # 生成今天的日报到 digests/YYYY-MM-DD.md
python3 -m http.server -d digests 8765   # 在浏览器里看(http://localhost:8765/)
```

零 pip 依赖——只用 Python 3 标准库(`urllib`/`json`/`xml.etree`),开箱即跑。需要 Python ≥3.9(`render.py` 用了 `str.removesuffix`)。

## 关注面配置

编辑 `config.json` 的 `keywords` 列表——只有标题/摘要命中至少一个关键词的条目才会上日报(见 `.claude/standards/skill-standards.md` 里蒸馏的「关键词关注面打分法」)。

## 数据来源(均为真实、免鉴权 API)

- [Hacker News API](https://github.com/HackerNews/API) — 社区热度信号
- [arXiv API](https://arxiv.org/help/api) — 学术前沿信号(cs.AI / cs.CL / cs.LG)

## 目录结构

```
aihot/
├── sources/hackernews.py   真实 HN 抓取
├── sources/arxiv.py        真实 arXiv 抓取
├── dedup.py                标题归一化去重
├── filter.py                关键词打分与过滤
├── render.py                Markdown + HTML 渲染
└── main.py                  CLI 编排入口
digests/                     生成的日报(按日期命名)
scripts/healthcheck.sh       一键健康检查
```

## 开发

这个项目由 Builders' Workbench 管理——五阶段方法论、Issue 驱动的活、真实运行记录,见 `.claude/standards/` 与 `PROJECT.md`。
