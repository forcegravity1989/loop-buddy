# 文档边界（写到哪、不写到哪）

> **30 秒导读**：给写文档和改 buddy 的人看。**现在作数**。当前运作：V3 修 bug、V4 规划特性。版本号与出包见 [`releases.md`](releases.md)；还没干的活只认 [`LEFTOVERS.md`](LEFTOVERS.md)。

看不懂的词查 [`../CONTEXT.md`](../CONTEXT.md)。

## 现在写哪一类

| 文件 / 目录 | 写什么 | 不写什么 |
|---|---|---|
| `plan/06`–`08` | 产品命题、铁律、内核设计。很少改，用户拍板才动 | 当周实践、安装器路径、版本出包流水 |
| `docs/vN-prototype/` | **该版本**的设计事实源（做之前写范围，做完改状态） | 把过程日记抄一遍；往过期版本目录堆新能力 |
| `docs/v1-prototype/`、`docs/v2-prototype/` | **史实**。设计当时怎么定的，现在仍可对照实现 | 新的未决、新的 V3/V4 设计 |
| `docs/v3-prototype/` | V3 范围内的设计与使用修复（含推广期修法） | V4 新特性 |
| `docs/v4-prototype/` | V4 特性规划（尚未成清单） | 假装已有范围；V3 的 bug 修法 |
| `docs/LEFTOVERS.md` | **还没干完的唯一清单**（开着的 + 已关的史实条目） | 当周叙事、设计正文 |
| `iterations/PRACTICE-buddy.md` | 实践过程：假设→动作→读回→结论；当周新发现 | 完整设计规格（只留指针）；第二份未决总表 |
| `docs/guide/` | 给同事用的产品指南（只写已实践） | 未实践功能、开发流水 |
| `docs/releases.md` | 版本号、出包时间、本版特性 / 修过的问题 | 设计论证 |
| `.claude/skills/` | agent 怎么干活 | 瞬时窗口号、当前遗留正文 |

## 落盘规则

1. **新特性** → 当前规划中的版本目录（今天是 `docs/v4-prototype/`）。动手前写设计篇，登记到该目录 README。走 `buddy-feature-dev`。
2. **V3 的 bug / 使用修复** → 设计落 `docs/v3-prototype/`（已有篇就改状态，不要新开平行文）。走 `buddy-bugfix`。
3. **还没干** → 只追加 `docs/LEFTOVERS.md`。PRACTICE §4 可以记当周发现，消化后迁进清单或关掉，不在 §4 养第二份总表。
4. **实践过程** → PRACTICE。已解的坑嵌进对应操作步；和后来实测矛盾就加「归正」，不悄悄改写。
5. **出一包** → 在 `docs/releases.md` 加一行：版本号、出包日、这一包相对上一包多了什么 / 修了什么。安装器 `AppVersion` 与这一行对齐。
6. **过时的版本目录** → 不删。顶部加横幅：「史实，遗留以 `docs/LEFTOVERS.md` 为准，当前运作见 `docs/releases.md`」。
7. **同一决定只在一处写正文**。别处只留指针。例如 V3-use-fix 的修法正文在 `docs/v3-prototype/onboard-list-and-claude-resolve.md`，PRACTICE 只指过去。

## 当前节奏（2026-08-17 起）

- **V3**：修 bug、推广期使用问题。可以排遗留清单里标了 V3 的条目（含 Cursor 执行器、cowelink 旁路）。
- **V4**：特性规划期。目录已开，清单未齐。多版本线等新能力进 V4 规划，不塞进 V3 安装包叙事。
- **不新开 V5 目录**，直到 V4 有了范围。
