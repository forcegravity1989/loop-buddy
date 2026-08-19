//! ISO 周:`2026-W34` 这种字符串与日期之间的换算。
//!
//! 周是 V4 的时间单位——周计划文件按周一份、活按周排、健康三条判据按周窗口
//! 算。这里只有换算,没有任何业务判断。

use time::{Date, Duration, OffsetDateTime, Weekday};

/// 某一天属于哪个 ISO 周,形如 `2026-W34`。
pub fn iso_week_of(date: Date) -> String {
    let (year, week, _) = date.to_iso_week_date();
    format!("{year}-W{week:02}")
}

/// 现在是哪个 ISO 周(UTC)。
pub fn current_week() -> String {
    iso_week_of(OffsetDateTime::now_utc().date())
}

/// `2026-W34` → 那一周的周一。认不出格式返回 `None`,不猜。
pub fn week_start(week: &str) -> Option<Date> {
    let (year, rest) = week.split_once("-W")?;
    let year: i32 = year.parse().ok()?;
    let week: u8 = rest.parse().ok()?;
    if !(1..=53).contains(&week) {
        return None;
    }
    // 从 1 月 4 日出发——ISO 规定它一定落在第 1 周。
    let jan4 = Date::from_calendar_date(year, time::Month::January, 4).ok()?;
    let monday_of_week1 = jan4 - Duration::days(jan4.weekday().number_days_from_monday() as i64);
    let candidate = monday_of_week1 + Duration::weeks(week as i64 - 1);
    // 第 53 周在多数年份不存在,换算回去对不上就说明这个周号是假的。
    (iso_week_of(candidate) == format!("{year}-W{week:02}")).then_some(candidate)
}

/// 那一周的周一 00:00 与下周一 00:00(左闭右开),给 git 按周窗口取数用。
pub fn week_bounds(week: &str) -> Option<(Date, Date)> {
    let start = week_start(week)?;
    Some((start, start + Duration::weeks(1)))
}

/// 上一周。认不出返回 `None`。
pub fn previous_week(week: &str) -> Option<String> {
    week_start(week).map(|d| iso_week_of(d - Duration::weeks(1)))
}

/// 一周里的周一是不是真的周一 —— 给读回用的自检。
pub fn starts_on_monday(week: &str) -> bool {
    week_start(week).is_some_and(|d| d.weekday() == Weekday::Monday)
}
