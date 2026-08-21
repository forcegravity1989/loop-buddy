# buddy 自带的技能库(唯一正本目录)

> **30 秒导读**:buddy 自带的全部技能住在这里 —— **一个目录、全部平级**,一个技能一个
> 子目录,里面一份 `SKILL.md`。开工时 buddy 把这些摊到**自己的资产目录**(不复制进用
> 户的仓),按活挂的 `workflow` 挑出对应那份,只把**名字 + 一句话 + 完整路径**写进
> agent 的系统提示词,正文让 agent 自己按需读 —— 渐进式加载,见
> `crates/bw-v4/src/standard/skills.rs`。给接手技能这块的人看。**现在作数**
> (2026-08-21 收敛:此前散在 `docs/skills/` 与 `standard/06-defaults/ops/` 两处、
> 还用目录层级表达「谁调用谁」;现在从属关系写在正文里,目录不表达任何结构)。

## 四份运作剧本(buddy 自己发起的标准动作)

| 活的 `workflow` 字段 | 目录 | 谁触发 | 一次会话大概 |
|---|---|---|---|
| `更新指标与周计划` | `week-planning/` | 人(总览横幅「开始本周」) | 20-40 分钟,人机 3-6 轮 |
| `资产盘点` | `asset-audit/` | 定时(默认周五 20:00)或接入老仓时一次 | 10-25 分钟,通常 0 轮 |
| —(子技能) | `metrics-refresh/` | `week-planning` 第二步指名调用 | 随宿主 |
| —(子技能) | `project-handbook/` | `asset-audit` 首次模式里**问过人**之后调用 | 随宿主 |

规范铺底(运作活③)**没有剧本** —— 它没有 agent 步骤,buddy 自己把核心件写进 `.bw/`。

## 七份方法论技能(按活的类别注入)

`evidence-first`(原型)· `spec-to-tests`(构建)· `baseline-before-touch`(优化)·
`fresh-eyes-funnel`(运营推广)· `breaking-drill`(运维)· `competitive-analysis` ·
`metrics-render`。

**两份没有收进来**:`north-star-discovery` 与 `metrics-binding` 的内容已并入
`metrics-refresh`,V4 的清单里不再有它们;`docs/skills/` 下的原文件留给 V3 用,
V4 不读那个目录。

## 版本

不单开版本线——版本号就是 `standard/VERSION`,随 buddy 整体发布走。改了内容 =
`standard/VERSION` 抬一档 + 这里记一行。项目侧 `.bw/managed.toml` 记这几份 SKILL.md
的指纹,规范对账能测出落后。

## 三条共同的规矩

1. **最远只到「评审中」**。这些活和所有 Issue 共用同一条铁律:agent 干完提 MR,
   状态最远推到「评审中」;「完成」永远是人自己点的那一下。
2. **不许伪造数据**。指标读数只能来自真实采集;采不到就如实说采不到,不为了让灯变绿
   手工改数、改定义。
3. **可能没人在场**。资产盘点是定时触发的——按「能做的先做、拿不准的写进报告等人看」
   推进,不要因为没人回应就卡住。
