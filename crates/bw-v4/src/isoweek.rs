//! ISO 周:`2026-W34` 这种字符串与日期之间的换算。
//!
//! 周是 V4 的时间单位——周计划文件按周一份、活按周排、健康三条判据按周窗口
//! 算。这里只有换算,没有任何业务判断。

use std::sync::OnceLock;
use time::{Date, Duration, OffsetDateTime, UtcOffset, Weekday};

/// 本机时区偏移。**必须在起任何线程之前调一次**(`main` / 指挥器的第一行):
/// Unix 上「当前时区」这个系统调用在多线程进程里不保证安全,`time` crate 因此
/// 只在单线程阶段才肯给准确答案。取不到就退回 UTC,并如实说一声。
///
/// 为什么值得费这个事:周是 V4 的时间单位,而中国用户在 UTC+8。周一早上
/// 八点前按 UTC 算还是上一周 —— 而周一早上正是人开周计划的时候。
static LOCAL_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

pub fn init_local_offset() {
    let offset = match UtcOffset::current_local_offset() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("[BW_TIME] 取不到本机时区,周按 UTC 算");
            UtcOffset::UTC
        }
    };
    let _ = LOCAL_OFFSET.set(offset);
}

/// 启动那一刻探到的本机时区偏移。**要格式化时间就用它**,不要再调一次
/// `UtcOffset::current_local_offset()` —— 那个调用在多线程进程里必然失败,
/// 静默退回 UTC,于是同一屏上时间戳按 UTC、周号按本机时区,差 8 小时。
pub fn local_offset() -> UtcOffset {
    *LOCAL_OFFSET.get().unwrap_or(&UtcOffset::UTC)
}

fn now_local() -> OffsetDateTime {
    OffsetDateTime::now_utc().to_offset(local_offset())
}

/// 某一天属于哪个 ISO 周,形如 `2026-W34`。
pub fn iso_week_of(date: Date) -> String {
    let (year, week, _) = date.to_iso_week_date();
    format!("{year}-W{week:02}")
}

/// 今天是几月几号,按本机时区算 —— 发版日这类「人眼里的今天」用它。
pub fn today_local() -> Date {
    now_local().date()
}

/// 现在是哪个 ISO 周,按**本机时区**算(见 [`init_local_offset`])。
pub fn current_week() -> String {
    iso_week_of(now_local().date())
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

/// 下一周。认不出返回 `None`。
pub fn next_week(week: &str) -> Option<String> {
    week_start(week).map(|d| iso_week_of(d + Duration::weeks(1)))
}

/// 一周里的周一是不是真的周一 —— 给读回用的自检。
pub fn starts_on_monday(week: &str) -> bool {
    week_start(week).is_some_and(|d| d.weekday() == Weekday::Monday)
}

/// `"fri 20:00"` → 星期几(周一 = 1)+ 时 + 分。认不出返回 `None`,不猜。
///
/// 只认这一种写法。cron 表达式不支持——`.bw/issue-policy.toml` 是给人读的,
/// 一周一次的节律不值得请一整套 cron 语法进来。
pub fn parse_schedule(spec: &str) -> Option<(u8, u8, u8)> {
    let spec = spec.trim().to_ascii_lowercase();
    let (day, time) = spec.split_once(char::is_whitespace)?;
    // **按字符取前三个,不是按字节**。`&day[..3]` 在 "mié 20:00"(第 4 个字节
    // 落在 é 中间)这种输入上直接 panic,而这条解析跑在内核线程里 —— 一次
    // panic 整条线程静默死掉,界面停在最后一帧,点什么都没反应。
    // 这个值是人手写进 `.bw/issue-policy.toml` 的,什么都可能写。
    let head: String = day.chars().take(3).collect();
    let dow = match head.as_str() {
        "mon" => 1,
        "tue" => 2,
        "wed" => 3,
        "thu" => 4,
        "fri" => 5,
        "sat" => 6,
        "sun" => 7,
        _ => return None,
    };
    let (h, m) = time.trim().split_once(':')?;
    let h: u8 = h.parse().ok()?;
    let m: u8 = m.parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some((dow, h, m))
}

/// 本周的这个时刻过了没有。
///
/// 判据只看「星期几 + 几点几分」在本周之内的先后,不算具体时间戳——因为幂等
/// 靠的是「本周有没有建过这张活」那条查询,这里只要回答「这一周里该触发的那
/// 一刻是不是已经过去了」。错过一次(比如那会儿 buddy 没开着)下次照样成立,
/// 自动补建,不需要另记一张「上次触发时间」的表。
pub fn schedule_passed_this_week(spec: &str) -> bool {
    let Some((dow, h, m)) = parse_schedule(spec) else {
        return false;
    };
    let now = now_local();
    let today = now.weekday().number_from_monday();
    (today, now.hour(), now.minute()) >= (dow, h, m)
}
