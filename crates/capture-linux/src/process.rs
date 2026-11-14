//! Whether an executable runs, from `/proc`.

/// How many leading arguments of a command line are looked at
const ARGS: usize = 3;

/// Whether a command line (`/proc/<pid>/cmdline`: the arguments, each ended by a NUL) is the executable's: one
/// of its first arguments is a path (Windows or Unix) ending in the executable's file name, case-insensitive.
/// Wine keeps the Windows path of the program as an argument: the first, or behind its loader.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn cmdline_runs(cmdline: &[u8], exe: &str) -> bool {
    cmdline.split(|b| *b == 0).take(ARGS).any(|arg| {
        String::from_utf8_lossy(arg).rsplit(['\\', '/']).next().is_some_and(|name| name.eq_ignore_ascii_case(exe))
    })
}

/// Whether a process of the executable runs (a Windows program under Wine too).
#[cfg(target_os = "linux")]
pub(crate) fn is_running(exe: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else { return false };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        // Process folders only
        name.as_encoded_bytes().iter().all(u8::is_ascii_digit)
            && std::fs::read(entry.path().join("cmdline")).is_ok_and(|cmdline| cmdline_runs(&cmdline, exe))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_by_the_file_name_of_a_leading_argument() {
        let sc = "StarCitizen.exe";
        // Wine: the Windows path first (spaces belong to the argument)
        assert!(cmdline_runs(b"C:\\Program Files\\RSI\\StarCitizen\\LIVE\\Bin64\\StarCitizen.exe\0--flag\0", sc));
        assert!(cmdline_runs(b"Z:\\games\\sc\\Bin64\\starcitizen.EXE\0", sc));
        // Behind a loader
        assert!(cmdline_runs(b"/opt/wine/bin/wine64-preloader\0/opt/wine/bin/wine64\0C:\\SC\\StarCitizen.exe\0", sc));
        // A Unix path, a bare name
        assert!(cmdline_runs(b"/games/sc/StarCitizen.exe\0", sc));
        assert!(cmdline_runs(b"StarCitizen.exe\0", sc));
    }

    #[test]
    fn other_programs_do_not_count() {
        let sc = "StarCitizen.exe";
        assert!(!cmdline_runs(b"C:\\Program Files\\RSI\\RSI Launcher\\RSI Launcher.exe\0--updated\0", sc));
        assert!(!cmdline_runs(b"C:\\Tools\\NotStarCitizen.exe\0", sc));
        // Named far behind, as an argument of something else
        assert!(!cmdline_runs(b"/usr/bin/tool\0-a\0-b\0C:\\SC\\StarCitizen.exe\0", sc));
        // A kernel thread has no command line
        assert!(!cmdline_runs(b"", sc));
    }
}
