//! Moviesmod catalog, season/episode pages and download-link pages.
mod parser;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{StreamExt, stream};
use reqwest::{StatusCode, Url};

use super::models::{
    CatalogItem, MediaDetails, PlaybackSource, ProviderError, ProviderKind, Release,
    ResolutionIntent,
};
use super::{Provider, ProviderCapabilities, ReleaseProvider, drive, site};

// Moviesmod rotates domains; MOVIEBOX_MOVIESMOD_URL can override this without a rebuild.
const DEFAULT_BASE_URL: &str = "https://moviesmod.ai.in/";
const DOMAIN_LIST: &str =
    "https://raw.githubusercontent.com/SaurabhKaperwan/Utils/refs/heads/main/urls.json";

#[derive(Clone)]
pub struct MoviesmodClient {
    client: reqwest::Client,
    domain: site::DomainSource,
    previous_domains: Vec<Url>,
}

impl MoviesmodClient {
    pub fn new() -> Result<Self, ProviderError> {
        let override_url = std::env::var("MOVIEBOX_MOVIESMOD_URL").ok();
        let mut client = Self::with_base_url(override_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
        if override_url.is_none() {
            client.domain = site::DomainSource::discovering(
                site::base_url(DEFAULT_BASE_URL)?,
                DOMAIN_LIST,
                "moviesmod",
            );
        }
        if let Ok(previous) = std::env::var("MOVIEBOX_MOVIESMOD_PREVIOUS_URL") {
            client.previous_domains.push(site::base_url(&previous)?);
        }
        Ok(client)
    }

    pub fn with_base_url(base: &str) -> Result<Self, ProviderError> {
        Ok(Self {
            client: site::browser_client()?,
            domain: site::DomainSource::fixed(site::base_url(base)?),
            previous_domains: vec![site::base_url(DEFAULT_BASE_URL)?],
        })
    }

    async fn post(
        &self,
        id: &str,
        only_season: Option<usize>,
    ) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
        let (requested, response) = self
            .domain
            .get(&self.client, |base| site::item_url(base, id), true)
            .await?;
        let base = site::returned_item_base(id, response.url())?;
        let html = response
            .error_for_status()
            .map_err(site::network_error)?
            .text()
            .await
            .map_err(site::network_error)?;
        site::check_page(&html)?;
        let mut aliases = self.domain.aliases(&requested).await;
        aliases.extend(self.previous_domains.iter().cloned());
        let (mut details, buttons) = parser::post_with_aliases(id, &base, &aliases, &html)?;
        self.domain.accept_redirect(&requested, &base).await;
        let is_series = details.is_series();
        let title = details.title.clone();
        let client = self.client.clone();
        // Each post can have multiple quality/season pages. A failed link does not hide the
        // rest of the post, and concurrency is bounded so we don't hammer the site.
        // If a post labels several distinct season pages, fetch only the selected one.
        // Otherwise a single intermediate page may contain *all* seasons: don't discard it.
        let restrict = only_season.is_some_and(|season| {
            buttons.iter().any(|button| button.season == season)
                && buttons.iter().any(|button| button.season != season)
        });
        let failed_pages = Arc::new(AtomicUsize::new(0));
        let pages = stream::iter(
            buttons
                .into_iter()
                .filter(|button| !restrict || Some(button.season) == only_season)
                .take(48),
        )
        .map(|button| {
            let client = client.clone();
            let title = title.clone();
            let failed_pages = failed_pages.clone();
            async move {
                if drive::is_media_url(&button.url) || drive::is_drive_page(&button.url) {
                    return parser::links(&button, &title, is_series, None);
                }
                let Ok(page) = link_page_retrying(&client, &button.url).await else {
                    failed_pages.fetch_add(1, Ordering::Relaxed);
                    return None;
                };
                if page.html.is_none() || drive::is_media_url(&page.url) {
                    let mut direct = button;
                    direct.url = page.url;
                    return parser::links(&direct, &title, is_series, None);
                }
                parser::links(
                    &button,
                    &title,
                    is_series,
                    Some((&page.url, page.html.as_deref().unwrap_or_default())),
                )
            }
        })
        .buffer_unordered(6)
        .filter_map(|result| async move { result })
        .collect::<Vec<_>>()
        .await;
        let mut releases: Vec<Release> = Vec::new();
        for release in pages.into_iter().flatten() {
            if let Some(existing) = releases.iter_mut().find(|item| {
                item.filename == release.filename
                    && item.season == release.season
                    && item.episode == release.episode
            }) {
                for mirror in release.mirrors {
                    if !existing
                        .mirrors
                        .iter()
                        .any(|m| m.resolver_url == mirror.resolver_url)
                    {
                        existing.mirrors.push(mirror);
                    }
                }
            } else {
                releases.push(release);
            }
        }
        // A post whose every page failed to load is a transient outage, not "no such release".
        if releases.is_empty() && failed_pages.load(Ordering::Relaxed) > 0 {
            return Err(ProviderError::Unavailable(
                "Moviesmod download pages did not load; try again in a moment".into(),
            ));
        }
        if is_series {
            details.seasons = site::seasons(&releases);
        }
        Ok((details, releases))
    }

    pub async fn resolve_release(
        &self,
        release: &Release,
        intent: ResolutionIntent,
    ) -> Result<PlaybackSource, ProviderError> {
        drive::resolve_release(&self.client, release, ProviderKind::Moviesmod, intent).await
    }
}

/// The site rate-limits bursts of page fetches; a failed page usually loads on the next try.
async fn link_page_retrying(
    client: &reqwest::Client,
    url: &reqwest::Url,
) -> Result<drive::Page, ProviderError> {
    let mut result = drive::link_page(client, url).await;
    for delay_ms in [300_u64, 900] {
        if result.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        result = drive::link_page(client, url).await;
    }
    result
}

impl Provider for MoviesmodClient {
    fn id(&self) -> ProviderKind {
        ProviderKind::Moviesmod
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_search: true,
            supports_pagination: true,
            supports_series: true,
            supports_subtitles: false,
            supports_homepage: false,
        }
    }

    async fn search(&self, query: &str, page: usize) -> Result<Vec<CatalogItem>, ProviderError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let (requested, response) = self
            .domain
            .get(
                &self.client,
                |base| {
                    let mut url = base.clone();
                    url.path_segments_mut()
                        .map_err(|_| ProviderError::Parsing("Invalid Moviesmod URL".into()))?
                        .pop_if_empty()
                        .extend(["search", query.trim(), "page", &page.max(1).to_string()]);
                    Ok(url)
                },
                page <= 1,
            )
            .await?;
        if response.status() == StatusCode::NOT_FOUND && page > 1 {
            // Exhausted search pages are WordPress 404s, not provider failures.
            return Ok(Vec::new());
        }
        let base = site::response_base(response.url())?;
        let html = response
            .error_for_status()
            .map_err(site::network_error)?
            .text()
            .await
            .map_err(site::network_error)?;
        site::check_page(&html)?;
        let mut aliases = self.domain.aliases(&requested).await;
        aliases.extend(self.previous_domains.iter().cloned());
        let items = parser::search_with_aliases(&base, &aliases, &html);
        if !items.is_empty() {
            self.domain.accept_redirect(&requested, &base).await;
        }
        Ok(items)
    }

    async fn details(&self, id: &str) -> Result<MediaDetails, ProviderError> {
        self.post(id, None).await.map(|(details, _)| details)
    }
}

impl ReleaseProvider for MoviesmodClient {
    async fn episode_streams(
        &self,
        id: &str,
        season: usize,
        episode: usize,
    ) -> Result<Vec<Release>, ProviderError> {
        let (_, releases) = self.post(id, (season > 0).then_some(season)).await?;
        Ok(releases
            .into_iter()
            .filter(|r| match (season, episode) {
                (0, 0) => r.season.is_none(),
                _ => r.season == Some(season) && r.episode == Some(episode),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn follows_intermediate_redirect_to_media_without_reading_the_video() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 1024];
                let n = stream.read(&mut request).await.unwrap();
                let line = String::from_utf8_lossy(&request[..n]);
                let path = line.split_whitespace().nth(1).unwrap_or("");
                let response = match path {
                    "/post/" => {
                        let body = "<meta property='og:title' content='Download Movie (2026)'><div class='thecontent'><a class='maxbutton-download-links' href='/go'>1080p</a></div>";
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                    }
                    "/go" => "HTTP/1.1 302 Found\r\nLocation: /video.mkv\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                    "/video.mkv" => "HTTP/1.1 200 OK\r\nContent-Type: video/x-matroska\r\nContent-Length: 500000000\r\nConnection: close\r\n\r\n".into(),
                    _ => panic!("Unexpected mock request: {path}"),
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let client = MoviesmodClient::with_base_url(&base).unwrap();
        let (details, releases) = client.post("/post/", None).await.unwrap();
        assert_eq!(
            details.media_type,
            crate::providers::models::MediaType::Movie
        );
        assert_eq!(releases.len(), 1);
        let media_url = format!("{base}/video.mkv");
        assert_eq!(releases[0].direct_url(), Some(media_url.as_str()));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn series_details_selected_episode_and_playback_use_intermediate_page() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0_u8; 1024];
                let count = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..count]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let (status, mime, body) = match path {
                    "/show/" => (
                        "200 OK",
                        "text/html",
                        "<meta property='og:title' content='Download Show Season 2 1080p'><div class='thecontent'><h3>Season 2 720p [1GB]</h3><p><a class='maxbutton-episode-links' href='/episodes/'>Episode Links</a></p></div>",
                    ),
                    "/episodes/" => (
                        "200 OK",
                        "text/html",
                        "<h3><a href='/first.mkv'>Episode 1</a></h3><h3><a href='/second.mkv'>Episode 2</a></h3>",
                    ),
                    "/second.mkv" => ("206 Partial Content", "video/x-matroska", "x"),
                    _ => panic!("Unexpected mock request: {path}"),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client = MoviesmodClient::with_base_url(&base).unwrap();
        let details = Provider::details(&client, "/show/").await.unwrap();
        assert_eq!(details.seasons[0].number, 2);
        assert_eq!(details.seasons[0].episodes.len(), 2);
        let releases = ReleaseProvider::episode_streams(&client, "/show/", 2, 2)
            .await
            .unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].quality.as_deref(), Some("720p"));
        assert_eq!(releases[0].episode, Some(2));
        let source = client
            .resolve_release(&releases[0], ResolutionIntent::Playback)
            .await
            .unwrap();
        assert_eq!(source.url, format!("{base}/second.mkv"));
        assert_eq!(source.provider, ProviderKind::Moviesmod);
        server.abort();
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn redirect_reuses_new_domain_and_rebases_old_search_cards() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let old_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let new_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let old = format!("http://{}", old_server.local_addr().unwrap());
        let new = format!("http://{}", new_server.local_addr().unwrap());
        let old_hits = Arc::new(AtomicUsize::new(0));
        let hits = old_hits.clone();
        let next = new.clone();
        let redirect = tokio::spawn(async move {
            loop {
                let (mut stream, _) = old_server.accept().await.unwrap();
                let mut buf = [0; 2048];
                let n = stream.read(&mut buf).await.unwrap();
                let line = String::from_utf8_lossy(&buf[..n]);
                let path = line.split_whitespace().nth(1).unwrap();
                hits.fetch_add(1, Ordering::SeqCst);
                let reply = format!(
                    "HTTP/1.1 301 Moved Permanently\r\nLocation: {next}{path}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let previous = old.clone();
        let content = tokio::spawn(async move {
            loop {
                let (mut stream, _) = new_server.accept().await.unwrap();
                let mut buf = [0; 2048];
                let n = stream.read(&mut buf).await.unwrap();
                let line = String::from_utf8_lossy(&buf[..n]);
                let path = line.split_whitespace().nth(1).unwrap();
                let body = if path == "/search/avatar/page/1" {
                    format!(
                        "<div class='post-cards'><article><a href='{previous}/film/' title='Download Film (2026)'><img src='/film.webp'></a></article></div>"
                    )
                } else if path == "/film/" {
                    format!(
                        "<meta property='og:title' content='Download Film (2026)'><div class='thecontent'><a class='maxbutton-download-links' href='{previous}/go'>1080p</a></div>"
                    )
                } else if path == "/go" {
                    "<a class='maxbutton-1' href='/video.mkv'>1080p 1 GB</a>".into()
                } else {
                    panic!("Unexpected request: {path}");
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client = MoviesmodClient::with_base_url(&old).unwrap();
        let items = Provider::search(&client, "avatar", 1).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.value, "/film/");
        assert_eq!(
            Provider::details(&client, "/film/").await.unwrap().id.value,
            "/film/"
        );
        let releases = ReleaseProvider::episode_streams(&client, "/film/", 0, 0)
            .await
            .unwrap();
        assert_eq!(releases.len(), 1);
        let media_url = format!("{new}/video.mkv");
        assert_eq!(releases[0].direct_url(), Some(media_url.as_str()));
        assert_eq!(old_hits.load(Ordering::SeqCst), 1);
        redirect.abort();
        content.abort();
    }

    #[tokio::test]
    async fn exhausted_search_page_is_empty_not_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0; 1024];
                let count = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..count]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let response = match path {
                    "/search/avatar/page/1" => {
                        let body = "<div class='post-cards'><article><a href='/download-avatar-2026/' title='Download Avatar (2026)'><img src='/avatar.webp'></a></article></div>";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    "/search/avatar/page/2" => {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    }
                    _ => panic!("Unexpected search request: {path}"),
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let client = MoviesmodClient::with_base_url(&base).unwrap();
        let results = Provider::search(&client, "avatar", 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.value, "/download-avatar-2026/");
        assert!(
            Provider::search(&client, "avatar", 2)
                .await
                .unwrap()
                .is_empty()
        );
        server.await.unwrap();
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Serves a post with one episode-links button; the episode page answers 503 until it has been
    /// asked `fail_first` times.
    async fn serve(fail_first: usize) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(AtomicUsize::new(0));
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0_u8; 1024];
                let count = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..count]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let (status, body) = match path {
                    "/show/" => (
                        "200 OK",
                        "<meta property='og:title' content='Download Show Season 2 1080p'><div class='thecontent'><h3>Season 2 720p [1GB]</h3><p><a class='maxbutton-episode-links' href='/episodes/'>Episode Links</a></p></div>",
                    ),
                    "/episodes/" if seen.fetch_add(1, Ordering::SeqCst) < fail_first => {
                        ("503 Service Unavailable", "busy")
                    }
                    "/episodes/" => (
                        "200 OK",
                        "<h3><a href='/first.mkv'>Episode 1</a></h3><h3><a href='/second.mkv'>Episode 2</a></h3>",
                    ),
                    _ => ("404 Not Found", ""),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (base, server)
    }

    #[tokio::test]
    async fn transient_link_page_failures_are_retried() {
        let (base, server) = serve(2).await;
        let client = MoviesmodClient::with_base_url(&base).unwrap();
        let releases = ReleaseProvider::episode_streams(&client, "/show/", 2, 2)
            .await
            .unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].episode, Some(2));
        server.abort();
    }

    #[tokio::test]
    async fn every_link_page_failing_is_an_error_not_an_empty_list() {
        let (base, server) = serve(usize::MAX).await;
        let client = MoviesmodClient::with_base_url(&base).unwrap();
        let result = ReleaseProvider::episode_streams(&client, "/show/", 2, 2).await;
        assert!(matches!(result, Err(ProviderError::Unavailable(_))));
        server.abort();
    }
}
