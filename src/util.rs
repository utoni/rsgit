pub fn format_time(unix_seconds: i64, offset_seconds: i32) -> String {
    let secs = unix_seconds + offset_seconds as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let off_abs = offset_seconds.unsigned_abs();
    let oh = off_abs / 3600;
    let om = (off_abs % 3600) / 60;
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} {sign}{oh:02}{om:02}")
}

pub fn format_date(unix_seconds: i64, offset_seconds: i32) -> String {
    let secs = unix_seconds + offset_seconds as i64;
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn relative(unix_seconds: i64, now: i64) -> String {
    let diff = (now - unix_seconds).max(0);
    let (n, unit) = if diff < 60 {
        (diff, "second")
    } else if diff < 3600 {
        (diff / 60, "minute")
    } else if diff < 86_400 {
        (diff / 3600, "hour")
    } else if diff < 2_592_000 {
        (diff / 86_400, "day")
    } else if diff < 31_536_000 {
        (diff / 2_592_000, "month")
    } else {
        (diff / 31_536_000, "year")
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

pub fn human_size(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

pub fn looks_binary(data: &[u8]) -> bool {
    let n = data.len().min(128);
    if n == 0 {
        return false;
    }
    if let Ok(s) = str::from_utf8(data) {
        let ok = s.chars().all(|c| !c.is_control() && !c.is_ascii_graphic());
        return ok;
    }
    data[..n].contains(&0)
}
