//! `.bw/metrics.toml` 的宽松读法:仓里这份文件缺 `[north_star]` 一节时,
//! 北极星如实降级为 `None`,不让整份正本一条都同步不进去(next 切片五-1
//! 修复轮,评审 task-s5a-review.md Important-4)。
//!
//! **移植件零改写**:[`crate::metrics_file::MetricsFile`] 是裁决 5 点名
//! 的零改写移植件,`north_star: NorthStarDef`(不是 `Option`)是它原本的
//! 强类型形状,这个文件一个字节都不碰它。design-s5-hexpanel.md §1.2 第
//! ①行钉死的产品语义却是「仓里没有这份文件 **/ 没有这一节** → 灰卡『北
//! 极星尚未定稿』」——今天的移植件没有 `#[serde(default)]`,缺这一节会
//! 让 `toml::from_str` 在解析阶段就整份失败,连滞后/引领也同步不进去,
//! §7.1 第 5 节那条验收步骤("删掉 `[north_star]` 再同步 → 显示尚未定
//! 稿灰")今天跑出来的是一个解析错误,不是灰卡。
//!
//! **怎么补,不碰移植件本体**:这里在移植件外面单独包一层——读文件前先
//! 探一次顶层有没有 `north_star` 这张表(不下钻判断节内字段对不对,节
//! 内字段错了照样交给移植件的强类型解析,让真实的语法/字段错误原样冒
//! 出来,这层不截胡)。探到了,原样转交 [`crate::metrics_file::read`],
//! 不重新发明一遍强类型解析;探不到,才走一条 `north_star` 允许缺席的
//! 单独解析路径,让滞后/引领照常同步。

use crate::metrics_file::{MetricDef, MetricsFileError, NorthStarDef, METRICS_FILE_REL_PATH};
use std::path::Path;

/// 与 [`crate::metrics_file::MetricsFile`] 同构,唯一的差别是 `north_star`
/// 允许缺席。`None` 只表示"这份文件没有 `[north_star]` 一节",不是"解析
/// 失败"——前者是这个字段的值,后者是 [`read_lenient`] 整个函数的 `Err`,
/// 两件事分得很清楚。
#[derive(Debug, Clone)]
pub struct LenientMetricsFile {
    pub schema_version: u32,
    pub north_star: Option<NorthStarDef>,
    pub lagging: Vec<MetricDef>,
    pub leading: Vec<MetricDef>,
}

/// 读 + 解析 `<workspace>/.bw/metrics.toml`,`[north_star]` 缺节时如实降级
/// 为 `None`。其余行为与 [`crate::metrics_file::read`] 一致:`Ok(None)` =
/// 没有这份文件(不区分"没配置"和"真没写"),真解析失败(语法错、节内
/// 字段缺失/类型不对)仍是 `Err`——不写一份半真的缓存,要么整份解析成功
/// (北极星允许是 `None`),要么整份都不同步。
pub fn read_lenient(workspace: &str) -> Result<Option<LenientMetricsFile>, MetricsFileError> {
    if workspace.trim().is_empty() {
        return Ok(None);
    }
    let path = Path::new(workspace).join(METRICS_FILE_REL_PATH);
    let display = path.display().to_string();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(MetricsFileError::Io {
                path: display,
                source: e,
            })
        }
    };

    // 只探"顶层有没有 north_star 这张表",不判断节内字段对不对——节内字
    // 段真的写错了,照样落进下面 `has_north_star` 为真的那条分支,原样
    // 交给移植件的强类型解析去报真实的 Parse 错误。顶层本身就不是合法
    // TOML,也不算"缺节"这一种情况,同样交给下面的严格路径。
    let has_north_star = match toml::from_str::<toml::Table>(&raw) {
        Ok(table) => table.contains_key("north_star"),
        Err(_) => true,
    };

    if has_north_star {
        return match crate::metrics_file::read(workspace)? {
            Some(f) => Ok(Some(LenientMetricsFile {
                schema_version: f.schema_version,
                north_star: Some(f.north_star),
                lagging: f.lagging,
                leading: f.leading,
            })),
            // 上面已经用 std::fs::read_to_string 确认文件存在,这一分支
            // 理论上不会命中;万一移植件未来自己的判断收紧了,如实转交
            // None,不在这里凭空造一个 Some。
            None => Ok(None),
        };
    }

    #[derive(serde::Deserialize)]
    struct NorthStarLessFile {
        #[serde(default)]
        schema_version: u32,
        #[serde(default)]
        lagging: Vec<MetricDef>,
        #[serde(default)]
        leading: Vec<MetricDef>,
    }

    let parsed: NorthStarLessFile = toml::from_str(&raw).map_err(|e| MetricsFileError::Parse {
        path: display,
        source: e,
    })?;
    Ok(Some(LenientMetricsFile {
        schema_version: parsed.schema_version,
        north_star: None,
        lagging: parsed.lagging,
        leading: parsed.leading,
    }))
}
