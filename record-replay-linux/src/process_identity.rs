use std::path::Path;

pub(crate) fn process_start_time_ticks(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }

    #[cfg(target_os = "linux")]
    {
        let stat =
            std::fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat")).ok()?;
        let (_, after_comm) = stat.rsplit_once(") ")?;
        // after_comm starts at procfs field 3 (state). Field 22 is starttime.
        after_comm.split_whitespace().nth(19)?.parse().ok()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Some(0)
    }
}

pub(crate) fn process_matches_start_time(pid: u32, expected_start_time_ticks: Option<u64>) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(target_os = "linux")]
    {
        let Some(current_start_time_ticks) = process_start_time_ticks(pid) else {
            return false;
        };
        expected_start_time_ticks.is_some_and(|expected| expected == current_start_time_ticks)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = expected_start_time_ticks;
        true
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn linux_process_identity_requires_start_time() {
        let pid = std::process::id();
        let start_time = process_start_time_ticks(pid);

        assert!(start_time.is_some());
        assert!(!process_matches_start_time(pid, None));
        assert!(process_matches_start_time(pid, start_time));
    }
}
