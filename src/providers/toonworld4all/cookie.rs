//! The ToonWorld4All archive cookie pasted in Settings. It is kept in its own file in the config
//! folder (owner-only on Unix) rather than in the shared settings file, and is only ever sent to
//! the archive's own redirect pages.
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "toonworld_cookie";
/// The archive's ad gate stays passed for about a day.
pub const COOKIE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCookie {
    pub value: String,
    /// Unix seconds when it was saved.
    pub saved_at: u64,
}

impl StoredCookie {
    pub fn age_secs(&self, now: u64) -> u64 {
        now.saturating_sub(self.saved_at)
    }

    pub fn is_stale(&self, now: u64) -> bool {
        self.age_secs(now) >= COOKIE_TTL_SECS
    }
}

static STORED: RwLock<Option<StoredCookie>> = RwLock::new(None);

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Cleans text pasted into the Settings field. `Ok(None)` means "clear it".
pub fn normalize(input: &str) -> Result<Option<String>, &'static str> {
    let mut text = input.trim();
    if let Some(prefix) = text.get(..7)
        && prefix.eq_ignore_ascii_case("cookie:")
    {
        text = text[7..].trim();
    }
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().any(char::is_control) {
        return Err("The cookie must be a single line.");
    }
    Ok(Some(text.to_string()))
}

/// The cookie from `MOVIEBOX_TOONWORLD_COOKIE`; it wins over the one saved in Settings.
pub fn from_env() -> Option<String> {
    std::env::var("MOVIEBOX_TOONWORLD_COOKIE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
}

pub fn current() -> Option<StoredCookie> {
    STORED.read().ok().and_then(|stored| stored.clone())
}

fn path() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join(FILE_NAME))
}

fn parse(content: &str) -> Option<StoredCookie> {
    let mut lines = content.lines();
    let saved_at = lines.next()?.trim().parse::<u64>().ok()?;
    let value = lines.next()?.trim();
    if value.is_empty() {
        return None;
    }
    Some(StoredCookie {
        value: value.to_string(),
        saved_at,
    })
}

fn read_from(path: &Path) -> Option<StoredCookie> {
    parse(&std::fs::read_to_string(path).ok()?)
}

fn write_to(path: &Path, cookie: &StoredCookie) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options.open(&temporary).and_then(|mut file| {
        file.write_all(format!("{}\n{}\n", cookie.saved_at, cookie.value).as_bytes())?;
        file.sync_all()
    });
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// Loads the saved cookie from disk into memory; called once at startup.
pub fn load() -> Option<StoredCookie> {
    let cookie = path().and_then(|path| read_from(&path));
    if let Ok(mut stored) = STORED.write() {
        *stored = cookie.clone();
    }
    cookie
}

/// Saves (or, with `None`, removes) the cookie and makes it active immediately.
pub fn save(value: Option<&str>) -> io::Result<Option<StoredCookie>> {
    let path = path().ok_or_else(|| io::Error::other("no config folder available"))?;
    let cookie = match value {
        Some(value) => {
            let cookie = StoredCookie {
                value: value.to_string(),
                saved_at: now_secs(),
            };
            write_to(&path, &cookie)?;
            Some(cookie)
        }
        None => {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            None
        }
    };
    if let Ok(mut stored) = STORED.write() {
        *stored = cookie.clone();
    }
    Ok(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_accepts_multibyte_text_near_the_prefix() {
        // Byte 7 falls inside "é"; this used to panic on the slice.
        let _ = normalize("abcdeé=1");
        let _ = normalize("ééééé");
    }

    #[test]
    fn normalize_trims_strips_the_header_name_and_rejects_line_breaks() {
        assert_eq!(
            normalize("  user=abc; x=1 "),
            Ok(Some("user=abc; x=1".into()))
        );
        assert_eq!(normalize("Cookie: user=abc"), Ok(Some("user=abc".into())));
        assert_eq!(normalize("cookie:user=abc"), Ok(Some("user=abc".into())));
        assert_eq!(normalize("   "), Ok(None));
        assert_eq!(normalize("cookie:  "), Ok(None));
        assert!(normalize("user=a\nSet-Cookie: b").is_err());
    }

    #[test]
    fn file_round_trips_and_is_private() {
        let dir = std::env::temp_dir().join(format!("moviebox-cookie-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let cookie = StoredCookie {
            value: "user=abc; x=1".into(),
            saved_at: 1_700_000_000,
        };
        write_to(&path, &cookie).unwrap();
        assert_eq!(read_from(&path), Some(cookie));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o077,
                0,
                "cookie file must not be group/world readable"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_rejects_malformed_files() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("not-a-number\nuser=a\n"), None);
        assert_eq!(parse("1700000000\n\n"), None);
    }

    #[test]
    fn staleness_follows_the_24_hour_gate() {
        let cookie = StoredCookie {
            value: "user=a".into(),
            saved_at: 1000,
        };
        assert!(!cookie.is_stale(1000 + COOKIE_TTL_SECS - 1));
        assert!(cookie.is_stale(1000 + COOKIE_TTL_SECS));
        assert_eq!(cookie.age_secs(10), 0);
    }
}
