use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, LoggerHandle, Naming, WriteMode};
use std::sync::{LazyLock, OnceLock};

static LOGGER_HANDLE: OnceLock<LoggerHandle> = OnceLock::new();
static HOME_DIR: LazyLock<Option<String>> =
    LazyLock::new(|| dirs::home_dir().and_then(|home| home.to_str().map(|home| home.to_string())));
pub fn init() {
    let default_level = "info";
    let spec = crate::env_vars::var("MOVIEBOX_LOG")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_level.to_string());

    let log_dir = crate::config::logs_dir();

    let logger_builder = Logger::try_with_str(&spec).unwrap_or_else(|error| {
        eprintln!(
            "[{}] invalid MOVIEBOX_LOG level '{}': {error}; falling back to '{}'",
            crate::config::APP_NAME,
            spec,
            default_level
        );
        Logger::try_with_str(default_level)
            .unwrap_or_else(|_| Logger::try_with_str("warn").unwrap())
    });

    match logger_builder
        .log_to_file(
            FileSpec::default()
                .directory(&log_dir)
                .basename(crate::config::APP_NAME),
        )
        .rotate(
            Criterion::Size(5 * 1024 * 1024),
            Naming::Numbers,
            Cleanup::KeepLogFiles(3),
        )
        .write_mode(WriteMode::BufferAndFlush)
        .format(flexi_logger::opt_format)
        .start()
    {
        Ok(handle) => {
            let _ = LOGGER_HANDLE.set(handle);
        }
        Err(error) => {
            eprintln!("[{}] logging unavailable: {error}", crate::config::APP_NAME);
        }
    }

    log::info!(
        "session started | {} {} | {} | log file: {}",
        crate::config::APP_NAME,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        display_path()
    );
}

pub fn flush() {
    if let Some(handle) = LOGGER_HANDLE.get() {
        handle.flush();
    }
}

pub fn log_file_path() -> std::path::PathBuf {
    crate::config::logs_dir().join(format!("{}_rCURRENT.log", crate::config::APP_NAME))
}

pub fn display_path() -> String {
    sanitize_path(log_file_path())
}

pub fn sanitize_path(path: impl AsRef<std::path::Path>) -> String {
    let raw = path.as_ref().to_string_lossy().into_owned();
    HOME_DIR
        .as_deref()
        .map(|home| raw.replacen(home, "~", 1))
        .unwrap_or(raw)
}

pub fn sanitize_url(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or("unknown");
            let scheme = parsed.scheme();
            if host.is_empty() {
                "[redacted]".to_string()
            } else if let Some(port) = parsed.port() {
                format!("{scheme}://{host}:{port}")
            } else {
                format!("{scheme}://{host}")
            }
        }
        Err(_) => {
            let lower = raw.to_ascii_lowercase();
            if let Some(idx) = lower.find("://") {
                let scheme = &raw[..idx];
                let rest = &raw[idx + 3..];
                let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
                let mut host = &rest[..host_end];
                if let Some(at_idx) = host.rfind('@') {
                    host = &host[at_idx + 1..];
                }
                if host.is_empty() {
                    "[redacted]".to_string()
                } else {
                    format!("{scheme}://{host}")
                }
            } else {
                "[redacted]".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_url_valid() {
        assert_eq!(
            sanitize_url("https://example.com/path?token=secret#hash"),
            "https://example.com"
        );
        assert_eq!(
            sanitize_url("http://custom.tv:8080/live/user/pass/123.m3u8"),
            "http://custom.tv:8080"
        );
    }

    #[test]
    fn test_sanitize_url_with_credentials() {
        assert_eq!(
            sanitize_url("https://user:pass@example.com/feed"),
            "https://example.com"
        );
        assert_eq!(
            sanitize_url("http://admin:secret@stream.local:8000/live.ts"),
            "http://stream.local:8000"
        );
    }

    #[test]
    fn test_sanitize_url_invalid() {
        assert_eq!(sanitize_url("not_a_url"), "[redacted]");
        assert_eq!(sanitize_url(""), "[redacted]");
    }
}
