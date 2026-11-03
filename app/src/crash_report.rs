//! Writes a crash report when the program panics. Nothing is caught: the default hook still
//! runs and the panic proceeds as before. The report holds the version, the panic message and
//! location, the thread and a backtrace. URL credentials are removed; no setting is read.

use std::path::PathBuf;

/// Folder for crash reports: `%LOCALAPPDATA%\<app>\crashes`, else the temp folder.
fn crash_folder(app_name: &str) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(app_name)
        .join("crashes")
}

/// The report text. Pure, so the format is tested without panicking.
pub(crate) fn format_report(unix_seconds: u64, thread: &str, message: &str, location: &str, backtrace: &str) -> String {
    format!(
        "GitCthulhu {} ({})\nTime (unix): {unix_seconds}\nThread: {thread}\nLocation: {location}\nMessage: {}\n\nBacktrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        gui_app::redact_credentials(message),
    )
}

pub(crate) fn install(app_name: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_string());
        let location = info
            .location()
            .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let folder = crash_folder(app_name);
        let report = format_report(now, &thread, &message, &location, &backtrace);
        if std::fs::create_dir_all(&folder).is_ok() {
            let file = folder.join(format!("crash-{now}.txt"));
            if std::fs::write(&file, report).is_ok() {
                eprintln!("crash report written to {}", file.display());
            }
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_names_version_thread_location_and_redacts_credentials() {
        let report = format_report(
            7,
            "main",
            "clone of https://me:secret@host/r.git failed",
            "src/x.rs:1:2",
            "frames",
        );
        assert!(report.starts_with(&format!("GitCthulhu {}", env!("CARGO_PKG_VERSION"))));
        assert!(report.contains("Thread: main"));
        assert!(report.contains("Location: src/x.rs:1:2"));
        assert!(report.contains("https://***@host/r.git"));
        assert!(!report.contains("secret"));
    }
}
