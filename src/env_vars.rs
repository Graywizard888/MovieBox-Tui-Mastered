//! Environment variables the app reads, with values saved in Settings (Env Variables). A value
//! set in the shell or on the command line (`MOVIEBOX_LOG=debug moviebox-tui`) wins; otherwise the
//! saved value applies, then the built-in default.
//! Values live in `env_vars` in the config folder as `NAME=value` lines, so they also reach the
//! seek-proxy sidecar process. Read them with [`var`] instead of `std::env::var`.
use std::collections::BTreeMap;
use std::env::VarError;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

const FILE_NAME: &str = "env_vars";

/// When a changed value takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// Read on every use.
    Now,
    /// Read once at startup.
    Restart,
}

/// What a value must look like before it is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// A whole number of zero or more.
    Number,
    /// An `http(s)://` origin.
    Url,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    pub applies: Applies,
}

const fn spec(name: &'static str, help: &'static str, kind: Kind, applies: Applies) -> Spec {
    Spec {
        name,
        help,
        kind,
        applies,
    }
}

/// Every variable that can be set from Settings. `MOVIEBOX_CONFIG_DIR` is left out because this
/// file lives in that folder, and `MOVIEBOX_TOONWORLD_COOKIE` has its own row in General.
pub const SPECS: &[Spec] = &[
    spec(
        "MOVIEBOX_SEEK_PROXY_MAX_MB",
        "Largest file (MB) that gets fake seeking when the host can't seek. Default 3000, 0 = off.",
        Kind::Number,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_SEEK_PROXY_END_WAIT_SECS",
        "Longest wait (s) for a player reading the end of an MKV while opening. Default 15.",
        Kind::Number,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_PLAYER",
        "Force a player: mpv, vlc, iina or android.",
        Kind::Text,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_MPV_PATH",
        "Path to the mpv executable.",
        Kind::Text,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_VLC_PATH",
        "Path to the VLC executable.",
        Kind::Text,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_IINA_PATH",
        "Path to the IINA executable (macOS).",
        Kind::Text,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_ANDROID_PLAYER_PATH",
        "Termux opener: path to termux-open, termux-open-url or termux-am.",
        Kind::Text,
        Applies::Now,
    ),
    spec(
        "MOVIEBOX_LOG",
        "Log level: off, error, warn, info, debug or trace. Default info.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_THEME",
        "Theme forced at launch, e.g. TokyoNight. Overrides the Theme tab.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_NO_IMAGE",
        "1 or true turns poster images off.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_IMAGE_PROTOCOL",
        "Force posters to kitty, sixel, iterm2, or off.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_CELL_SIZE",
        "Font cell size in pixels for posters, e.g. 10x20.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_DATA_DIR",
        "Folder for watch history and favorites.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_CACHE_DIR",
        "Folder for the disk cache.",
        Kind::Text,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_FOURKHDHUB_URL",
        "4KHDHub address when its domain changes.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_UHDMOVIES_URL",
        "UHDMovies address when its domain changes.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_UHDMOVIES_PREVIOUS_URL",
        "Old UHDMovies address that cards still link to.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_MOVIESMOD_URL",
        "Moviesmod address when its domain changes.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_MOVIESMOD_PREVIOUS_URL",
        "Old Moviesmod address that cards still link to.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_TOONWORLD4ALL_URL",
        "ToonWorld4All site address.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_TOONWORLD4ALL_PREVIOUS_URL",
        "Old ToonWorld4All site address used in posts.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_TOONWORLD4ALL_ARCHIVE_URL",
        "ToonWorld4All archive address.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_TOONWORLD4ALL_PREVIOUS_ARCHIVE_URL",
        "Old ToonWorld4All archive address.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_TOONWORLD4ALL_WORKER_URL",
        "ToonWorld4All redirect worker address.",
        Kind::Url,
        Applies::Restart,
    ),
    spec(
        "MOVIEBOX_TOONWORLD4ALL_PREVIOUS_WORKER_URL",
        "Old ToonWorld4All redirect worker address.",
        Kind::Url,
        Applies::Restart,
    ),
];

static SAVED: RwLock<BTreeMap<String, String>> = RwLock::new(BTreeMap::new());

/// Like `std::env::var`, falling back to the value saved in Settings when the shell has none.
pub fn var(name: &str) -> Result<String, VarError> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        shell => saved(name).ok_or(()).or(shell),
    }
}

/// The value saved in Settings, if any.
pub fn saved(name: &str) -> Option<String> {
    SAVED.read().ok()?.get(name).cloned()
}

/// The shell's value, ignoring Settings.
pub fn shell(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Trims and checks a typed value. `Ok(None)` means "clear it".
pub fn normalize(spec: &Spec, input: &str) -> Result<Option<String>, String> {
    let value = input.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().any(char::is_control) {
        return Err("The value must be a single line.".into());
    }
    match spec.kind {
        Kind::Text => {}
        Kind::Number => {
            if value.parse::<u64>().is_err() {
                return Err(format!("{} needs a whole number.", spec.name));
            }
        }
        Kind::Url => {
            let ok = reqwest::Url::parse(value)
                .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.has_host());
            if !ok {
                return Err(format!("{} needs an https:// address.", spec.name));
            }
        }
    }
    Ok(Some(value.to_string()))
}

fn path() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join(FILE_NAME))
}

/// Parses `NAME=value` lines, keeping only known names. Blank lines and `#` comments are skipped.
fn parse(content: &str) -> BTreeMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            let (name, value) = (name.trim(), value.trim());
            let known = SPECS.iter().any(|spec| spec.name == name);
            (known && !value.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

fn render(values: &BTreeMap<String, String>) -> String {
    let mut out =
        String::from("# Saved from Settings > Env Variables. A value set in the shell wins.\n");
    for (name, value) in values {
        out.push_str(name);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

fn write_to(path: &Path, values: &BTreeMap<String, String>) -> io::Result<()> {
    if values.is_empty() {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, render(values))?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// Loads saved values into memory. Call once at startup, before anything reads a variable.
pub fn load() {
    let values = path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|content| parse(&content))
        .unwrap_or_default();
    if let Ok(mut saved) = SAVED.write() {
        *saved = values;
    }
}

/// Saves (or, with `None`, removes) one value and makes it active immediately.
pub fn save(name: &str, value: Option<&str>) -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::other("no config folder available"))?;
    let mut next = SAVED.read().map(|saved| saved.clone()).unwrap_or_default();
    match value {
        Some(value) => next.insert(name.to_string(), value.to_string()),
        None => next.remove(name),
    };
    write_to(&path, &next)?;
    if let Ok(mut saved) = SAVED.write() {
        *saved = next;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_are_unique_and_documented() {
        for (i, spec) in SPECS.iter().enumerate() {
            assert!(spec.name.starts_with("MOVIEBOX_"));
            assert!(!spec.help.is_empty());
            assert!(SPECS[..i].iter().all(|other| other.name != spec.name));
        }
        assert!(SPECS.iter().all(|spec| spec.name != "MOVIEBOX_CONFIG_DIR"));
    }

    #[test]
    fn shell_value_wins_over_saved_value() {
        let name = "MOVIEBOX_TEST_ENV_VARS_PRECEDENCE";
        SAVED
            .write()
            .unwrap()
            .insert(name.to_string(), "saved".to_string());
        assert_eq!(var(name).as_deref(), Ok("saved"));
        // SAFETY: the name is unique to this test, so no other thread reads or writes it.
        unsafe { std::env::set_var(name, "shell") };
        assert_eq!(var(name).as_deref(), Ok("shell"));
        // An empty shell value doesn't hide the saved one.
        unsafe { std::env::set_var(name, "") };
        assert_eq!(var(name).as_deref(), Ok("saved"));
        unsafe { std::env::remove_var(name) };
        SAVED.write().unwrap().remove(name);
        assert!(var(name).is_err());
    }

    #[test]
    fn parse_keeps_known_names_and_skips_comments() {
        let parsed = parse(
            "# note\nMOVIEBOX_SEEK_PROXY_MAX_MB = 4000\nUNKNOWN=1\nMOVIEBOX_LOG=\n\nbroken line\n",
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["MOVIEBOX_SEEK_PROXY_MAX_MB"], "4000");
    }

    #[test]
    fn file_round_trips_and_empty_removes_it() {
        let dir = std::env::temp_dir().join(format!("moviebox-env-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let mut values = BTreeMap::new();
        values.insert(
            "MOVIEBOX_UHDMOVIES_URL".to_string(),
            "https://a.example/?x=1".to_string(),
        );
        write_to(&path, &values).unwrap();
        assert_eq!(parse(&std::fs::read_to_string(&path).unwrap()), values);
        write_to(&path, &BTreeMap::new()).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_checks_numbers_and_urls() {
        let number = SPECS[0];
        assert_eq!(normalize(&number, " 4000 "), Ok(Some("4000".into())));
        assert_eq!(normalize(&number, "  "), Ok(None));
        assert!(normalize(&number, "4 GB").is_err());
        let url = spec("X", "x", Kind::Url, Applies::Restart);
        assert!(normalize(&url, "https://uhdmovies.example/").is_ok());
        assert!(normalize(&url, "uhdmovies.example").is_err());
        let text = spec("X", "x", Kind::Text, Applies::Now);
        assert!(normalize(&text, "a\u{7}b").is_err());
    }
}
