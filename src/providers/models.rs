use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ProviderKind {
    #[default]
    #[serde(rename = "moviebox", alias = "movie_box")]
    MovieBox,
    #[serde(rename = "fourkhdhub", alias = "four_k_hd_hub", alias = "4khdhub")]
    FourKHdHub,
    #[serde(rename = "bdix_circleftp", alias = "bdix_circle_ftp")]
    BdixCircleFtp,
    #[serde(rename = "bdix_dhakaflix", alias = "bdix_dhaka_flix")]
    BdixDhakaFlix,
    #[serde(rename = "addons", alias = "addon")]
    Addons,
    #[serde(rename = "dramachi")]
    Dramachi,
    #[serde(rename = "uhdmovies", alias = "uhd_movies")]
    UhdMovies,
    #[serde(rename = "moviesmod")]
    Moviesmod,
    #[serde(rename = "toonworld4all", alias = "toon_world4all")]
    ToonWorld4All,
}

impl ProviderKind {
    pub const ENABLED: [Self; 8] = [
        Self::MovieBox,
        Self::FourKHdHub,
        Self::UhdMovies,
        Self::Moviesmod,
        Self::ToonWorld4All,
        Self::Dramachi,
        Self::BdixCircleFtp,
        Self::BdixDhakaFlix,
    ];

    pub const fn cache_key(self) -> &'static str {
        match self {
            Self::MovieBox => "moviebox",
            Self::FourKHdHub => "fourkhdhub",
            Self::BdixCircleFtp => "bdix_circleftp",
            Self::BdixDhakaFlix => "bdix_dhakaflix",
            Self::Addons => "addons",
            Self::Dramachi => "dramachi",
            Self::UhdMovies => "uhdmovies",
            Self::Moviesmod => "moviesmod",
            Self::ToonWorld4All => "toonworld4all",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MovieBox => "MovieBox",
            Self::FourKHdHub => "4KHDHub",
            Self::BdixCircleFtp => "CircleFTP (BDIX)",
            Self::BdixDhakaFlix => "DhakaFlix (BDIX)",
            Self::Addons => "Addons",
            Self::Dramachi => "Dramachi",
            Self::UhdMovies => "UHDMovies",
            Self::Moviesmod => "Moviesmod",
            Self::ToonWorld4All => "ToonWorld4All",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "moviebox" => Some(Self::MovieBox),
            "4khdhub" | "fourkhdhub" => Some(Self::FourKHdHub),
            "bdix_circleftp" | "circleftp (bdix)" => Some(Self::BdixCircleFtp),
            "bdix_dhakaflix" | "dhakaflix (bdix)" => Some(Self::BdixDhakaFlix),
            "addons" | "addon" => Some(Self::Addons),
            "dramachi" => Some(Self::Dramachi),
            "uhdmovies" | "uhd_movies" => Some(Self::UhdMovies),
            "moviesmod" => Some(Self::Moviesmod),
            "toonworld4all" | "toon_world4all" => Some(Self::ToonWorld4All),
            _ => None,
        }
    }

    pub const fn is_bdix(self) -> bool {
        matches!(self, Self::BdixCircleFtp | Self::BdixDhakaFlix)
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderMediaId {
    pub provider: ProviderKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestContext {
    pub provider: ProviderKind,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Movie,
    Series,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: ProviderMediaId,
    pub title: String,
    pub media_type: MediaType,
    pub year: Option<String>,
    pub poster_url: Option<String>,
    pub season_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub season: usize,
    pub number: usize,
    pub title: Option<String>,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Season {
    pub number: usize,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTrackOption {
    pub subject_id: String,
    pub language: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaDetails {
    pub id: ProviderMediaId,
    pub title: String,
    pub media_type: MediaType,
    pub year: Option<String>,
    pub description: Option<String>,
    pub tagline: Option<String>,
    pub imdb_rating: Option<String>,
    pub director: Option<String>,
    pub stars: Option<String>,
    pub prints: Option<String>,
    pub audios: Option<String>,
    pub poster_url: Option<String>,
    pub duration: Option<String>,
    pub genres: Vec<String>,
    pub seasons: Vec<Season>,
    pub dubs: Vec<AudioTrackOption>,
}

impl MediaDetails {
    pub fn is_series(&self) -> bool {
        self.media_type == MediaType::Series || !self.seasons.is_empty()
    }

    pub fn has_languages(&self) -> bool {
        self.dubs.len() > 1
    }

    pub fn cover_url(&self) -> Option<&str> {
        self.poster_url.as_deref()
    }

    pub fn sibling_ids(&self) -> Vec<String> {
        let mut ids = vec![self.id.value.clone()];
        ids.extend(self.dubs.iter().map(|dub| dub.subject_id.clone()));
        ids.retain(|s| !s.is_empty());
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn from_search_result(
        item: &crate::models::SearchResult,
        preview: Option<&MediaDetails>,
    ) -> Self {
        if let Some(p) = preview.filter(|p| p.id.value == item.id && p.id.provider == item.provider)
        {
            let mut details = p.clone();
            if details.title.trim().is_empty() {
                details.title = item.title.clone();
            }
            if details.year.is_none() && !item.release_year.trim().is_empty() {
                details.year = Some(item.release_year.clone());
            }
            if details.poster_url.is_none() {
                details.poster_url = item.cover_url.clone();
            }
            details
        } else {
            MediaDetails {
                id: ProviderMediaId {
                    provider: item.provider,
                    value: item.id.clone(),
                },
                title: item.title.clone(),
                media_type: if item.stype == 2 {
                    MediaType::Series
                } else {
                    MediaType::Movie
                },
                year: if !item.release_year.trim().is_empty() {
                    Some(item.release_year.clone())
                } else {
                    None
                },
                description: None,
                tagline: None,
                imdb_rating: None,
                director: None,
                stars: None,
                prints: None,
                audios: None,
                poster_url: item.cover_url.clone(),
                duration: None,
                genres: vec![],
                seasons: vec![],
                dubs: vec![],
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionIntent {
    Playback,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMirror {
    pub label: String,
    pub resolver_url: String,
    pub headers: Vec<(String, String)>,
    pub direct_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleOption {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub provider: ProviderKind,
    pub filename: String,
    pub quality: Option<String>,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub size_bytes: Option<u64>,
    pub season: Option<usize>,
    pub episode: Option<usize>,
    pub mirrors: Vec<SourceMirror>,
    #[serde(default)]
    pub resource_id: Option<String>,
}
impl Release {
    fn unknown_resolution(&self) -> bool {
        self.quality.is_none()
            && matches!(
                self.provider,
                ProviderKind::UhdMovies | ProviderKind::Moviesmod | ProviderKind::ToonWorld4All
            )
    }

    pub fn is_multi_resolution(&self) -> bool {
        self.quality
            .as_deref()
            .is_some_and(|q| q.eq_ignore_ascii_case("multi") || q.eq_ignore_ascii_case("multi-res"))
    }

    pub fn resolution_u64(&self) -> u64 {
        if self.unknown_resolution() {
            return 0;
        }
        self.quality
            .as_deref()
            .and_then(|q| {
                let trimmed = q.trim();
                if trimmed.eq_ignore_ascii_case("4k") || trimmed.eq_ignore_ascii_case("uhd") {
                    return Some(2160);
                }
                trimmed.trim_end_matches(['p', 'P']).parse::<u64>().ok()
            })
            .unwrap_or(1080)
    }

    pub fn resolution_i64(&self) -> i64 {
        if self.unknown_resolution() {
            -2 // No stated quality: do not invent a 1080p release.
        } else if self.is_multi_resolution() {
            -1
        } else {
            self.resolution_u64() as i64
        }
    }
    pub fn source_label(&self) -> &str {
        self.mirrors
            .first()
            .map(|m| m.label.as_str())
            .unwrap_or_else(|| match self.provider {
                ProviderKind::FourKHdHub => "4KHDHub",
                ProviderKind::BdixCircleFtp => "CircleFTP",
                ProviderKind::BdixDhakaFlix => "DhakaFlix",
                ProviderKind::Addons => "Addon",
                ProviderKind::Dramachi => "Dramachi",
                ProviderKind::UhdMovies => "UHDMovies",
                ProviderKind::Moviesmod => "Moviesmod",
                ProviderKind::ToonWorld4All => "ToonWorld4All",
                ProviderKind::MovieBox => "Direct",
            })
    }

    pub fn direct_url(&self) -> Option<&str> {
        self.mirrors.first().map(|m| m.resolver_url.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackSource {
    pub provider: ProviderKind,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub subtitle: Option<String>,
    pub source_label: String,
    #[serde(default)]
    pub max_height: Option<u64>,
}

impl PlaybackSource {
    pub fn bare(provider: ProviderKind, url: impl Into<String>, subtitle: Option<String>) -> Self {
        Self {
            provider,
            url: url.into(),
            headers: Vec::new(),
            subtitle,
            source_label: provider.label().to_string(),
            max_height: None,
        }
    }
}
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderError {
    #[error("Network connection failed: {0}")]
    Network(String),
    #[error("Rate limited by provider")]
    RateLimited(Option<u64>),
    #[error("Item not found on provider")]
    NotFound,
    #[error("Failed to parse response: {0}")]
    Parsing(String),
    #[error("Provider is temporarily unavailable: {0}")]
    Unavailable(String),
}

impl ProviderError {
    pub fn user_message(&self, provider: ProviderKind) -> String {
        let label = match provider {
            ProviderKind::BdixCircleFtp => "CircleFTP",
            ProviderKind::BdixDhakaFlix => "DhakaFlix",
            _ => provider.label(),
        };

        match self {
            Self::Network(msg) => {
                if provider.is_bdix() {
                    format!("{label} unreachable: requires BDIX network.")
                } else {
                    let lower = msg.to_ascii_lowercase();
                    if lower.contains("timed out") || lower.contains("timeout") {
                        format!("{label} timed out.")
                    } else {
                        format!("Cannot reach {label}.")
                    }
                }
            }
            Self::RateLimited(secs) => match secs {
                Some(s) => format!("Rate limited. Wait {s}s."),
                None => "Rate limited. Try later.".to_string(),
            },
            Self::NotFound => "No results found.".to_string(),
            Self::Parsing(_) => format!("{label} parse error."),
            Self::Unavailable(msg) => {
                let trimmed = msg.trim();
                if let Some(status) = trimmed.strip_prefix("HTTP status ") {
                    format!("{label} error ({status}).")
                } else if trimmed.is_empty() {
                    format!("{label} unavailable.")
                } else {
                    format!("{label} unavailable: {trimmed}")
                }
            }
        }
    }
}

impl From<String> for ProviderError {
    fn from(msg: String) -> Self {
        Self::Unavailable(msg)
    }
}

impl From<&str> for ProviderError {
    fn from(msg: &str) -> Self {
        Self::Unavailable(msg.to_string())
    }
}

pub fn strip_emojis(input: &str) -> String {
    input
        .chars()
        .filter(|&c| {
            let u = c as u32;
            !((0x1F000..=0x1FAFF).contains(&u)
                || (0x2600..=0x27BF).contains(&u)
                || (0x2300..=0x23FF).contains(&u)
                || (0x2B00..=0x2BFF).contains(&u)
                || (0xFE00..=0xFE0F).contains(&u)
                || u == 0x200D)
        })
        .collect::<String>()
}

pub fn clean_stream_text(input: &str) -> String {
    let without_emojis = strip_emojis(input);
    let mut cleaned = String::new();
    let mut last_was_space = false;
    for c in without_emojis.chars() {
        if c.is_whitespace() {
            if !last_was_space && !cleaned.is_empty() {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(c);
            last_was_space = false;
        }
    }
    cleaned.trim().to_string()
}

pub fn extract_4digit_year(raw: &str) -> String {
    raw.as_bytes()
        .windows(4)
        .find(|window| window.iter().all(u8::is_ascii_digit) && matches!(window[0], b'1' | b'2'))
        .and_then(|window| std::str::from_utf8(window).ok())
        .map(str::to_string)
        .unwrap_or_default()
}

/// Pixel height named by a release label or filename. Whole tokens only: substring matching
/// turns site tags (`4KHDHub`), encoder tags (`DS4K`) and flags (`HDR`, `SDR`) into bogus
/// 4K/720p/480p tags. The first explicit height wins (`1080p ... 4K UHD` is 1080p); a bare
/// `4K` token is only a fallback and the generic word `UHD` is never enough (UHDMovies and
/// 4KHDHub brand 1080p encodes "1080p UHD"). Bare numbers such as `1080` count here because
/// release filenames use them (`Video.1080.WEB-DL`).
pub fn label_resolution(label: &str) -> Option<u64> {
    scan_resolution(label, true)
}

/// Like [`label_resolution`] for free-form titles, where a bare number is just a word
/// ("360", "The 480 Club", "1408"): only explicit `p`/`i` heights or a `4K` token count.
pub fn title_resolution(title: &str) -> Option<u64> {
    scan_resolution(title, false)
}

fn scan_resolution(text: &str, allow_bare: bool) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    let mut has_4k_token = false;
    for token in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
        let height = match token {
            "2160p" | "2160i" => 2160,
            "1440p" => 1440,
            "1080p" | "1080i" => 1080,
            "720p" => 720,
            "576p" => 576,
            "540p" => 540,
            "480p" => 480,
            "360p" => 360,
            "2160" if allow_bare => 2160,
            "1440" if allow_bare => 1440,
            "1080" if allow_bare => 1080,
            "720" if allow_bare => 720,
            "576" if allow_bare => 576,
            "540" if allow_bare => 540,
            "480" if allow_bare => 480,
            "4k" => {
                has_4k_token = true;
                continue;
            }
            _ => continue,
        };
        return Some(height);
    }
    has_4k_token.then_some(2160)
}

pub fn parse_size_bytes(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    for i in 0..parts.len() {
        let clean = parts[i].trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
        if let Ok(num) = clean.parse::<f64>()
            && i + 1 < parts.len()
        {
            let unit = parts[i + 1]
                .trim_matches(|c: char| !c.is_ascii_alphabetic())
                .to_ascii_uppercase();
            match unit.as_str() {
                "TB" | "TIB" | "T" => return Some((num * 1_099_511_627_776.0) as u64),
                "GB" | "GIB" | "G" => return Some((num * 1_073_741_824.0) as u64),
                "MB" | "MIB" | "M" => return Some((num * 1_048_576.0) as u64),
                "KB" | "KIB" | "K" => return Some((num * 1_024.0) as u64),
                _ => {}
            }
        }
        let upper = clean.to_ascii_uppercase();
        for (suffix, multiplier) in [
            ("TIB", 1_099_511_627_776.0),
            ("TB", 1_099_511_627_776.0),
            ("GIB", 1_073_741_824.0),
            ("GB", 1_073_741_824.0),
            ("MIB", 1_048_576.0),
            ("MB", 1_048_576.0),
            ("KIB", 1_024.0),
            ("KB", 1_024.0),
        ] {
            if let Some(num_str) = upper.strip_suffix(suffix)
                && let Ok(num) = num_str.parse::<f64>()
            {
                return Some((num * multiplier) as u64);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordpress_provider_ids_round_trip() {
        for (provider, key) in [
            (ProviderKind::UhdMovies, "uhdmovies"),
            (ProviderKind::Moviesmod, "moviesmod"),
            (ProviderKind::ToonWorld4All, "toonworld4all"),
        ] {
            assert_eq!(provider.cache_key(), key);
            assert_eq!(ProviderKind::parse(provider.label()), Some(provider));
            assert_eq!(
                serde_json::to_string(&provider).unwrap(),
                format!("\"{key}\"")
            );
            assert_eq!(
                serde_json::from_str::<ProviderKind>(&format!("\"{key}\"")).unwrap(),
                provider
            );
        }
    }

    #[test]
    fn test_release_resolution_parsing() {
        let make_release = |q: Option<&str>| Release {
            provider: ProviderKind::BdixCircleFtp,
            filename: "Test.mkv".to_string(),
            quality: q.map(|s| s.to_string()),
            codec: None,
            language: None,
            size_bytes: None,
            season: None,
            episode: None,
            mirrors: Vec::new(),
            resource_id: None,
        };

        assert_eq!(make_release(Some("4K")).resolution_u64(), 2160);
        assert_eq!(make_release(Some("4k")).resolution_u64(), 2160);
        assert_eq!(make_release(Some("2160p")).resolution_u64(), 2160);
        assert_eq!(make_release(Some("1080p")).resolution_u64(), 1080);
        assert_eq!(make_release(None).resolution_u64(), 1080);
        for provider in [
            ProviderKind::UhdMovies,
            ProviderKind::Moviesmod,
            ProviderKind::ToonWorld4All,
        ] {
            let mut release = make_release(None);
            release.provider = provider;
            assert_eq!(release.resolution_u64(), 0);
            assert_eq!(release.resolution_i64(), -2);
        }
    }

    #[test]
    fn test_provider_error_user_message_concise() {
        let bdix_net_err = ProviderError::Network("tcp connect error: operation timed out".into());
        assert_eq!(
            bdix_net_err.user_message(ProviderKind::BdixCircleFtp),
            "CircleFTP unreachable: requires BDIX network."
        );
        assert_eq!(
            bdix_net_err.user_message(ProviderKind::BdixDhakaFlix),
            "DhakaFlix unreachable: requires BDIX network."
        );

        let timeout_err = ProviderError::Network("operation timed out".into());
        assert_eq!(
            timeout_err.user_message(ProviderKind::MovieBox),
            "MovieBox timed out."
        );

        let connect_err = ProviderError::Network("dns lookup failed".into());
        assert_eq!(
            connect_err.user_message(ProviderKind::FourKHdHub),
            "Cannot reach 4KHDHub."
        );

        let rate_limit = ProviderError::RateLimited(Some(30));
        assert_eq!(
            rate_limit.user_message(ProviderKind::MovieBox),
            "Rate limited. Wait 30s."
        );

        let not_found = ProviderError::NotFound;
        assert_eq!(
            not_found.user_message(ProviderKind::MovieBox),
            "No results found."
        );

        let http_status = ProviderError::Unavailable("HTTP status 502".into());
        assert_eq!(
            http_status.user_message(ProviderKind::FourKHdHub),
            "4KHDHub error (502)."
        );
    }

    #[test]
    fn test_parse_size_bytes_formats_and_units() {
        assert_eq!(parse_size_bytes("1.5 GB"), Some(1_610_612_736));
        assert_eq!(parse_size_bytes("2 GiB"), Some(2_147_483_648));
        assert_eq!(parse_size_bytes("450MB"), Some(471_859_200));
        assert_eq!(parse_size_bytes("512 KB"), Some(524_288));
        assert_eq!(parse_size_bytes("1 TB"), Some(1_099_511_627_776));
        assert_eq!(parse_size_bytes("💾 2.25 GB ⚡"), Some(2_415_919_104));
        assert_eq!(parse_size_bytes("No size here"), None);
        assert_eq!(parse_size_bytes(""), None);
    }

    #[test]
    fn test_media_details_sibling_ids_deduplicates_and_sorts() {
        let details = MediaDetails {
            id: ProviderMediaId {
                provider: ProviderKind::MovieBox,
                value: "sub_main".to_string(),
            },
            title: "Test".to_string(),
            media_type: MediaType::Series,
            year: None,
            description: None,
            tagline: None,
            imdb_rating: None,
            director: None,
            stars: None,
            prints: None,
            audios: None,
            poster_url: None,
            duration: None,
            genres: vec![],
            seasons: vec![],
            dubs: vec![
                AudioTrackOption {
                    subject_id: "sub_hi".to_string(),
                    language: "Hindi".to_string(),
                    label: "Hindi".to_string(),
                },
                AudioTrackOption {
                    subject_id: "sub_main".to_string(),
                    language: "English".to_string(),
                    label: "English".to_string(),
                },
                AudioTrackOption {
                    subject_id: String::new(),
                    language: "Empty".to_string(),
                    label: "Empty".to_string(),
                },
            ],
        };
        assert_eq!(
            details.sibling_ids(),
            vec!["sub_hi".to_string(), "sub_main".to_string()]
        );
    }
}
