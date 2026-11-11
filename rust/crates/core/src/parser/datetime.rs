//! E-H 页面日期与时间戳换算。与 Dart 版 `DateFormat`/`DateTime.toString` 输出一致。

/// `dd MMMM yyyy, HH:mm` (en_US) -> `yyyy-MM-dd HH:mm`
pub(crate) fn format_eh_datetime(raw: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
        "December",
    ];
    let (date, time) = raw.split_once(", ")?;
    let mut parts = date.split(' ');
    let day: u32 = parts.next()?.trim().parse().ok()?;
    let month_name = parts.next()?;
    let year: i32 = parts.next()?.trim().parse().ok()?;
    let month = MONTHS.iter().position(|m| *m == month_name)? + 1;
    let (hour, minute) = time.trim().split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    Some(format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"))
}


/// Howard Hinnant 的 days -> civil date 算法（`DateTime` 的日历换算）
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    #[test]
    fn datetime_matches_dart_intl() {
        assert_eq!(super::format_eh_datetime("10 March 2022, 03:49").as_deref(), Some("2022-03-10 03:49"));
        assert_eq!(super::format_eh_datetime("1 January 2020, 23:05").as_deref(), Some("2020-01-01 23:05"));
    }
}
