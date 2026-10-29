//! Commit dates as the log shows them: "Today 15:18", "Yesterday 09:02", "28 Oct 2026".
//!
//! Pure and in UTC. The crate has no time-zone source, and a zone read from the machine
//! would make the same commit read differently on two machines. Labels say "UTC".

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const SECS_PER_DAY: i64 = 86_400;

/// `(year, month 1–12, day 1–31)` for days since 1970-01-01 (Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, usize, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as usize, day)
}

fn clock(secs: i64) -> String {
    let in_day = secs.rem_euclid(SECS_PER_DAY);
    format!("{:02}:{:02}", in_day / 3_600, (in_day % 3_600) / 60)
}

fn calendar(secs: i64) -> String {
    let (year, month, day) = civil_from_days(secs.div_euclid(SECS_PER_DAY));
    format!("{day} {} {year}", MONTHS[month - 1])
}

/// The log's date column: today and yesterday with the time, older commits by date.
pub(crate) fn log_date(now: i64, then: i64) -> String {
    let days_ago = now.div_euclid(SECS_PER_DAY) - then.div_euclid(SECS_PER_DAY);
    match days_ago {
        0 => format!("Today {}", clock(then)),
        1 => format!("Yesterday {}", clock(then)),
        _ => calendar(then),
    }
}

/// Full date and time, for the commit details.
pub(crate) fn full_date(then: i64) -> String {
    format!("{} {} UTC", calendar(then), clock(then))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-28 15:18:00 UTC.
    const NOW: i64 = 1_793_200_680;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(calendar(NOW), "28 Oct 2026");
        assert_eq!(calendar(-1), "31 Dec 1969");
    }

    #[test]
    fn recent_commits_show_the_time_older_ones_the_date() {
        assert_eq!(log_date(NOW, NOW - 60), "Today 15:17");
        assert_eq!(log_date(NOW, NOW - SECS_PER_DAY), "Yesterday 15:18");
        assert_eq!(log_date(NOW, NOW - 3 * SECS_PER_DAY), "25 Oct 2026");
    }

    #[test]
    fn full_date_names_the_zone() {
        assert_eq!(full_date(NOW), "28 Oct 2026 15:18 UTC");
    }
}
