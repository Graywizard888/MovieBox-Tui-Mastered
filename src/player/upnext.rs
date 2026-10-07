//! "Up next" sidecar used by the mpv overlay script (`scripts/mpv/up_next.lua`).
//!
//! Network streams have no directory for a player-side script to scan, so the
//! application publishes the next episode of the active series to a small JSON
//! file. The mpv script reads it to render the "Up Next" overlay and, when a
//! playable URL is present, to autoplay the next episode.
//!
//! Android intent launches cannot carry `--script-opts`, and `mpv-android`
//! cannot read Termux's private data directory, so the sidecar is mirrored into
//! shared storage on Android.

use crate::providers::models::Season;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SIDECAR_FILE: &str = "upnext.json";
pub const REQUEST_FILE: &str = "upnext_request.json";
const SIDECAR_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentEpisode {
    pub season: usize,
    pub episode: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextEpisode {
    pub has_next: bool,
    pub season: usize,
    pub episode: usize,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Resolved, directly playable URL. When absent the player script asks the
    /// application to resolve and relaunch instead of loading it itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpNextSidecar {
    pub version: u32,
    pub updated_at: u64,
    pub provider: String,
    pub subject_id: String,
    pub title: String,
    pub current: CurrentEpisode,
    pub next: NextEpisode,
    #[serde(default)]
    pub request_file: String,
}

/// Request written by the player script when it wants the application to play
/// the next episode (used when no direct URL was published).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayNextRequest {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub subject_id: String,
    pub season: usize,
    pub episode: usize,
    #[serde(default)]
    pub requested_at: u64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn episode_label(season: usize, episode: usize) -> String {
    format!("S{season:02}E{episode:02}")
}

/// Directories the sidecar is published to. The private data directory is
/// always first; Android shared storage is added so `mpv-android` (a separate
/// app sandbox) can read it.
pub fn sidecar_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(dir) = crate::config::playback_state_dir() {
        dirs.push(dir);
    }

    if crate::updater::artifact::is_termux_environment() && !crate::config::is_test_environment() {
        for shared in [PathBuf::from("/storage/emulated/0"), PathBuf::from("/sdcard")] {
            if shared.is_dir() {
                dirs.push(shared.join("MovieBox-TUI"));
            }
        }
        if let Some(home) = dirs::home_dir() {
            let shared = home.join("storage").join("shared");
            if shared.is_dir() {
                dirs.push(shared.join("MovieBox-TUI"));
            }
        }
    }

    dirs
}

pub fn sidecar_paths() -> Vec<PathBuf> {
    sidecar_dirs()
        .into_iter()
        .map(|dir| dir.join(SIDECAR_FILE))
        .collect()
}

pub fn request_paths() -> Vec<PathBuf> {
    sidecar_dirs()
        .into_iter()
        .map(|dir| dir.join(REQUEST_FILE))
        .collect()
}

/// Path advertised to the player script for writing a "play next" request.
/// Prefers shared storage so Android players can reach it.
pub fn advertised_request_path() -> Option<PathBuf> {
    let paths = request_paths();
    paths.last().cloned().or_else(|| paths.first().cloned())
}

/// Primary sidecar path, used for the mpv `--script-opts` hint on desktop.
pub fn primary_sidecar_path() -> Option<PathBuf> {
    sidecar_paths().into_iter().next()
}

/// Resolve the episode that follows `(season, episode)` within `seasons`.
/// Rolls over into the next season when the current one is exhausted.
pub fn compute_next(
    seasons: &[Season],
    season: usize,
    episode: usize,
) -> Option<(usize, usize, Option<String>)> {
    if seasons.is_empty() {
        return None;
    }

    let season_idx = seasons.iter().position(|s| s.number == season)?;
    let current = seasons.get(season_idx)?;

    if let Some(pos) = current.episodes.iter().position(|e| e.number == episode)
        && let Some(next) = current.episodes.get(pos + 1)
    {
        return Some((current.number, next.number, next.title.clone()));
    }

    for upcoming in seasons.iter().skip(season_idx + 1) {
        if let Some(first) = upcoming.episodes.first() {
            return Some((upcoming.number, first.number, first.title.clone()));
        }
    }

    None
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    provider: &str,
    subject_id: &str,
    title: &str,
    seasons: &[Season],
    season: usize,
    episode: usize,
    current_url: Option<String>,
) -> UpNextSidecar {
    let next = match compute_next(seasons, season, episode) {
        Some((next_season, next_episode, next_title)) => NextEpisode {
            has_next: true,
            season: next_season,
            episode: next_episode,
            label: episode_label(next_season, next_episode),
            title: next_title.filter(|t| !t.trim().is_empty()),
            url: None,
            subtitle: None,
            headers: Vec::new(),
        },
        None => NextEpisode::default(),
    };

    UpNextSidecar {
        version: SIDECAR_VERSION,
        updated_at: now_secs(),
        provider: provider.to_string(),
        subject_id: subject_id.to_string(),
        title: title.to_string(),
        current: CurrentEpisode {
            season,
            episode,
            url: current_url,
        },
        next,
        request_file: advertised_request_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

/// Publish the sidecar to every known location. Failures are logged, never
/// fatal: the overlay is cosmetic and must not break playback.
pub fn publish(sidecar: &UpNextSidecar) {
    let Ok(json) = serde_json::to_vec_pretty(sidecar) else {
        return;
    };
    for path in sidecar_paths() {
        if let Err(error) = crate::cache::atomic_write_file(&path, &json) {
            log::debug!(
                "failed to write up-next sidecar to {}: {error}",
                crate::logging::sanitize_path(&path)
            );
        }
    }
}

/// Mark that nothing follows the current item (movies, final episodes), so the
/// overlay reports "Season Ended" instead of showing a stale next episode.
pub fn publish_absent(
    provider: &str,
    subject_id: &str,
    title: &str,
    season: usize,
    episode: usize,
) {
    publish(&UpNextSidecar {
        version: SIDECAR_VERSION,
        updated_at: now_secs(),
        provider: provider.to_string(),
        subject_id: subject_id.to_string(),
        title: title.to_string(),
        current: CurrentEpisode {
            season,
            episode,
            url: None,
        },
        next: NextEpisode::default(),
        request_file: advertised_request_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    });
}

/// Consume a pending "play next" request written by the player script.
/// The request file is always removed, even when it cannot be parsed.
pub fn take_request() -> Option<PlayNextRequest> {
    let mut found: Option<PlayNextRequest> = None;
    for path in request_paths() {
        if !path.exists() {
            continue;
        }
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<PlayNextRequest>(&content).ok());
        let _ = std::fs::remove_file(&path);
        if found.is_none()
            && let Some(request) = parsed
            && request.episode > 0
        {
            found = Some(request);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::models::Episode;

    fn season(number: usize, episodes: &[usize]) -> Season {
        Season {
            number,
            episodes: episodes
                .iter()
                .map(|&n| Episode {
                    season: number,
                    number: n,
                    title: Some(format!("Episode {n}")),
                    overview: None,
                })
                .collect(),
        }
    }

    #[test]
    fn next_episode_within_season() {
        let seasons = vec![season(1, &[1, 2, 3])];
        let next = compute_next(&seasons, 1, 2).expect("next episode");
        assert_eq!((next.0, next.1), (1, 3));
        assert_eq!(next.2.as_deref(), Some("Episode 3"));
    }

    #[test]
    fn next_episode_rolls_into_following_season() {
        let seasons = vec![season(1, &[1, 2]), season(2, &[5, 6])];
        let next = compute_next(&seasons, 1, 2).expect("next season");
        assert_eq!((next.0, next.1), (2, 5));
    }

    #[test]
    fn no_next_episode_after_final_entry() {
        let seasons = vec![season(1, &[1, 2])];
        assert!(compute_next(&seasons, 1, 2).is_none());
        assert!(compute_next(&[], 1, 1).is_none());
    }

    #[test]
    fn unknown_season_has_no_next_episode() {
        let seasons = vec![season(1, &[1, 2])];
        assert!(compute_next(&seasons, 4, 1).is_none());
    }

    #[test]
    fn labels_are_zero_padded() {
        assert_eq!(episode_label(1, 5), "S01E05");
        assert_eq!(episode_label(12, 134), "S12E134");
    }

    #[test]
    fn build_marks_absence_of_next_episode() {
        let seasons = vec![season(1, &[1])];
        let sidecar = build("moviebox", "42", "Example", &seasons, 1, 1, None);
        assert!(!sidecar.next.has_next);
        assert_eq!(sidecar.current.episode, 1);
        assert_eq!(sidecar.version, SIDECAR_VERSION);
    }

    #[test]
    fn build_describes_next_episode() {
        let seasons = vec![season(1, &[1, 2])];
        let sidecar = build(
            "moviebox",
            "42",
            "Example",
            &seasons,
            1,
            1,
            Some("https://stream.test/1".to_string()),
        );
        assert!(sidecar.next.has_next);
        assert_eq!(sidecar.next.label, "S01E02");
        assert_eq!(sidecar.current.url.as_deref(), Some("https://stream.test/1"));
    }

    #[test]
    fn sidecar_round_trips_through_json() {
        let seasons = vec![season(1, &[1, 2])];
        let sidecar = build("moviebox", "42", "Example", &seasons, 1, 1, None);
        let json = serde_json::to_string(&sidecar).expect("serialize");
        let parsed: UpNextSidecar = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, sidecar);
    }

    #[test]
    fn request_parses_player_payload() {
        let payload =
            r#"{"provider":"moviebox","subject_id":"42","season":1,"episode":2,"requested_at":10}"#;
        let request: PlayNextRequest = serde_json::from_str(payload).expect("parse request");
        assert_eq!(request.episode, 2);
        assert_eq!(request.subject_id, "42");
    }
}
