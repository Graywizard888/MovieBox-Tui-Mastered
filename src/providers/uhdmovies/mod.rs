//! UHDMovies WordPress catalog and G-Drive releases.
//! The release URL is a landing page, not the media URL; `drive` resolves it at play time.
mod parser;

use reqwest::{StatusCode, Url};

use super::models::{
    CatalogItem, MediaDetails, PlaybackSource, ProviderError, ProviderKind, Release,
    ResolutionIntent,
};
use super::{Provider, ProviderCapabilities, ReleaseProvider, drive, site};

const DEFAULT_BASE_URL: &str = "https://uhdmovies.my/";
const DOMAIN_LIST: &str =
    "https://raw.githubusercontent.com/phisher98/TVVVV/refs/heads/main/domains.json";

#[derive(Clone)]
pub struct UhdMoviesClient {
    client: reqwest::Client,
    domain: site::DomainSource,
    previous_domains: Vec<Url>,
}

impl UhdMoviesClient {
    pub fn new() -> Result<Self, ProviderError> {
        let override_url = std::env::var("MOVIEBOX_UHDMOVIES_URL").ok();
        let mut client = Self::with_base_url(override_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
        if override_url.is_none() {
            client.domain = site::DomainSource::discovering(
                site::base_url(DEFAULT_BASE_URL)?,
                DOMAIN_LIST,
                "UHDMovies",
            );
        }
        if let Ok(previous) = std::env::var("MOVIEBOX_UHDMOVIES_PREVIOUS_URL") {
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

    async fn post(&self, id: &str) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
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
        let post = parser::post_with_aliases(id, &base, &aliases, &html)?;
        // A successful post, not a third-party redirect/gate, confirms the new origin.
        self.domain.accept_redirect(&requested, &base).await;
        Ok(post)
    }

    pub async fn resolve_release(
        &self,
        release: &Release,
        intent: ResolutionIntent,
    ) -> Result<PlaybackSource, ProviderError> {
        drive::resolve_release(&self.client, release, ProviderKind::UhdMovies, intent).await
    }
}

impl Provider for UhdMoviesClient {
    fn id(&self) -> ProviderKind {
        ProviderKind::UhdMovies
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
                    let mut url = if page > 1 {
                        base.join(&format!("page/{page}/"))
                            .map_err(|_| ProviderError::Parsing("Invalid search page".into()))?
                    } else {
                        base.clone()
                    };
                    url.query_pairs_mut().append_pair("s", query.trim());
                    Ok(url)
                },
                page <= 1,
            )
            .await?;
        if response.status() == StatusCode::NOT_FOUND && page > 1 {
            // WordPress returns 404 after the last search-results page.
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
        self.post(id).await.map(|(details, _)| details)
    }
}

impl ReleaseProvider for UhdMoviesClient {
    async fn episode_streams(
        &self,
        id: &str,
        season: usize,
        episode: usize,
    ) -> Result<Vec<Release>, ProviderError> {
        let (_, releases) = self.post(id).await?;
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn redirected_domain_rebases_old_permalinks_and_is_reused() {
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
                    "HTTP/1.1 302 Found\r\nLocation: {next}{path}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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
                let body = if path.starts_with("/?s=runner") {
                    format!(
                        "<article class='gridlove-post'><div class='entry-image'><a href='{previous}/film/'><img src='/poster.webp'></a></div><h2>Download Film (2026)</h2></article>"
                    )
                } else if path == "/film/" {
                    format!(
                        "<h1>Download Film (2026)</h1><div class='entry-content'><a href='{previous}/?sid=token'>1080p UHD</a></div>"
                    )
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
        let client = UhdMoviesClient::with_base_url(&old).unwrap();
        let items = Provider::search(&client, "runner", 1).await.unwrap();
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
        let link = format!("{new}/?sid=token");
        assert_eq!(releases[0].direct_url(), Some(link.as_str()));
        assert_eq!(old_hits.load(Ordering::SeqCst), 1);
        redirect.abort();
        content.abort();
    }

    #[tokio::test]
    async fn live_search_shape_and_exhausted_page() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0; 1024];
                let count = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..count]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let response = match path {
                    "/?s=runner" => "HTTP/1.1 302 Found\r\nLocation: /search/runner/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    "/search/runner/" => {
                        let body = "<article class='gridlove-post'><div class='entry-image'><a href='/download-the-runner-2026/'><img src='/poster.webp'></a></div><h2>Download The Runner (2026) 2160p</h2></article>";
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                    }
                    "/page/2/?s=runner" => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    _ => panic!("Unexpected search request: {path}"),
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let client = UhdMoviesClient::with_base_url(&base).unwrap();
        let results = Provider::search(&client, "runner", 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.value, "/download-the-runner-2026/");
        assert!(
            Provider::search(&client, "runner", 2)
                .await
                .unwrap()
                .is_empty()
        );
        server.await.unwrap();
    }
}
