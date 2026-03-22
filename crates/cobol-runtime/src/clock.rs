use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct RuntimeClock {
    base_unix_cs: i128,
    start: Instant,
    scale: i128,
}

fn read_time_scale() -> i128 {
    std::env::var("COBOL_TEST_FAST_TIME_SCALE")
        .ok()
        .and_then(|value| value.trim().parse::<i128>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
}

fn runtime_clock() -> &'static RuntimeClock {
    static CLOCK: OnceLock<RuntimeClock> = OnceLock::new();
    CLOCK.get_or_init(|| RuntimeClock {
        base_unix_cs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i128
            / 10,
        start: Instant::now(),
        scale: read_time_scale(),
    })
}

fn current_unix_centis() -> i128 {
    let clock = runtime_clock();
    if clock.scale == 1 {
        return SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i128
            / 10;
    }

    let elapsed_cs = clock.start.elapsed().as_millis() as i128 / 10;
    clock.base_unix_cs + elapsed_cs * clock.scale
}

/// Fill the current local date/time components used by ACCEPT FROM DATE/TIME.
///
/// # Safety
/// Any non-null output pointer must be valid for writing one `i32`.
#[no_mangle]
pub unsafe extern "C" fn cobol_runtime_now_parts(
    year_ptr: *mut i32,
    month_ptr: *mut i32,
    day_ptr: *mut i32,
    yday1_ptr: *mut i32,
    wday_mon1_ptr: *mut i32,
    hour_ptr: *mut i32,
    minute_ptr: *mut i32,
    sec_centis_ptr: *mut i32,
) {
    let unix_cs = current_unix_centis();
    let unix_secs = (unix_cs / 100) as libc::time_t;
    let centis = (unix_cs.rem_euclid(100)) as i32;

    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    libc::localtime_r(&unix_secs, tm.as_mut_ptr());
    let tm = tm.assume_init();

    if !year_ptr.is_null() {
        *year_ptr = tm.tm_year + 1900;
    }
    if !month_ptr.is_null() {
        *month_ptr = tm.tm_mon + 1;
    }
    if !day_ptr.is_null() {
        *day_ptr = tm.tm_mday;
    }
    if !yday1_ptr.is_null() {
        *yday1_ptr = tm.tm_yday + 1;
    }
    if !wday_mon1_ptr.is_null() {
        *wday_mon1_ptr = if tm.tm_wday == 0 { 7 } else { tm.tm_wday };
    }
    if !hour_ptr.is_null() {
        *hour_ptr = tm.tm_hour;
    }
    if !minute_ptr.is_null() {
        *minute_ptr = tm.tm_min;
    }
    if !sec_centis_ptr.is_null() {
        *sec_centis_ptr = tm.tm_sec * 100 + centis;
    }
}

#[cfg(test)]
mod tests {
    use super::cobol_runtime_now_parts;

    #[test]
    fn test_cobol_runtime_now_parts_returns_valid_ranges() {
        let mut year = 0;
        let mut month = 0;
        let mut day = 0;
        let mut yday = 0;
        let mut wday = 0;
        let mut hour = 0;
        let mut minute = 0;
        let mut sec_centis = 0;

        unsafe {
            cobol_runtime_now_parts(
                &mut year,
                &mut month,
                &mut day,
                &mut yday,
                &mut wday,
                &mut hour,
                &mut minute,
                &mut sec_centis,
            );
        }

        assert!(year >= 2000);
        assert!((1..=12).contains(&month));
        assert!((1..=31).contains(&day));
        assert!((1..=366).contains(&yday));
        assert!((1..=7).contains(&wday));
        assert!((0..=23).contains(&hour));
        assert!((0..=59).contains(&minute));
        assert!((0..=5999).contains(&sec_centis));
    }
}
