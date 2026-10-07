//! ToonWorld4All WordPress catalog, episode archive and link validation.
//! Search/details use the public WP API when available; HTML posts are a fallback. Movie
//! mirrors and archive redirects are not media URLs and are resolved only when selected.
pub mod cookie;
mod parser;
mod resolver;

use reqwest::{StatusCode, Url};

use super::models::{
    CatalogItem, MediaDetails, PlaybackSource, ProviderError, ProviderKind, Release,
    ResolutionIntent,
};
use super::{Provider, ProviderCapabilities, ReleaseProvider, site};
use parser::Origins;

const DEFAULT_BASE: &str = "https://toonworld4all.me/";
const DEFAULT_ARCHIVE: &str = "https://archive.toonworld4all.me/";
const DEFAULT_WORKER: &str = "https://backend.tw4all.workers.dev/";
const MAX_POST_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct ToonWorld4AllClient {
    client: reqwest::Client,
    no_redirect: reqwest::Client,
    site_domain: site::DomainSource,
    archive_domain: site::DomainSource,
    worker: Url,
    site_aliases: Vec<Url>,
    archive_aliases: Vec<Url>,
    worker_aliases: Vec<Url>,
}

impl ToonWorld4AllClient {
    pub fn new() -> Result<Self, ProviderError> {
        let base =
            std::env::var("MOVIEBOX_TOONWORLD4ALL_URL").unwrap_or_else(|_| DEFAULT_BASE.into());
        let archive = std::env::var("MOVIEBOX_TOONWORLD4ALL_ARCHIVE_URL")
            .unwrap_or_else(|_| DEFAULT_ARCHIVE.into());
        let worker = std::env::var("MOVIEBOX_TOONWORLD4ALL_WORKER_URL")
            .unwrap_or_else(|_| DEFAULT_WORKER.into());
        let mut client = Self::with_endpoints(&base, &archive, &worker)?;
        for (key, aliases) in [
            (
                "MOVIEBOX_TOONWORLD4ALL_PREVIOUS_URL",
                &mut client.site_aliases,
            ),
            (
                "MOVIEBOX_TOONWORLD4ALL_PREVIOUS_ARCHIVE_URL",
                &mut client.archive_aliases,
            ),
            (
                "MOVIEBOX_TOONWORLD4ALL_PREVIOUS_WORKER_URL",
                &mut client.worker_aliases,
            ),
        ] {
            if let Ok(previous) = std::env::var(key) {
                aliases.push(site::base_url(&previous)?);
            }
        }
        Ok(client)
    }

    pub fn with_base_url(base: &str) -> Result<Self, ProviderError> {
        Self::with_endpoints(base, DEFAULT_ARCHIVE, DEFAULT_WORKER)
    }

    fn with_endpoints(base: &str, archive: &str, worker: &str) -> Result<Self, ProviderError> {
        Ok(Self {
            client: site::browser_client()?,
            no_redirect: crate::net::http_client_builder()
                .user_agent(crate::net::DEFAULT_BROWSER_USER_AGENT)
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(18))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(site::network_error)?,
            site_domain: site::DomainSource::fixed(site::base_url(base)?),
            archive_domain: site::DomainSource::fixed(site::base_url(archive)?),
            worker: site::base_url(worker)?,
            site_aliases: vec![site::base_url(DEFAULT_BASE)?],
            archive_aliases: vec![site::base_url(DEFAULT_ARCHIVE)?],
            worker_aliases: vec![site::base_url(DEFAULT_WORKER)?],
        })
    }

    async fn origins(&self) -> Origins {
        let site = self.site_domain.base(&self.client).await;
        let archive = self.archive_domain.base(&self.client).await;
        let mut site_aliases = self.site_domain.aliases(&site).await;
        site_aliases.extend(self.site_aliases.iter().cloned());
        let mut archive_aliases = self.archive_domain.aliases(&archive).await;
        archive_aliases.extend(self.archive_aliases.iter().cloned());
        Origins {
            site,
            archive,
            worker: self.worker.clone(),
            site_aliases,
            archive_aliases,
            worker_aliases: self.worker_aliases.clone(),
        }
    }

    async fn get_text(&self, url: Url) -> Result<(Url, StatusCode, String), ProviderError> {
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(site::network_error)?;
        let status = response.status();
        let effective_url = response.url().clone();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_POST_BYTES as u64)
        {
            return Err(ProviderError::Parsing("Provider page is too large".into()));
        }
        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(site::network_error)? {
            if data.len() + chunk.len() > MAX_POST_BYTES {
                return Err(ProviderError::Parsing("Provider page is too large".into()));
            }
            data.extend_from_slice(&chunk);
        }
        let text = String::from_utf8_lossy(&data).into_owned();
        site::check_page(&text)?;
        Ok((effective_url, status, text))
    }

    fn api_url(base: &Url) -> Result<Url, ProviderError> {
        base.join("wp-json/wp/v2/posts")
            .map_err(|_| ProviderError::Parsing("Invalid WordPress API URL".into()))
    }

    async fn post(&self, id: &str) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
        let mut origins = self.origins().await;
        let requested = origins.site.clone();
        let url = site::item_url(&requested, id)?;
        // WordPress permalinks normally end with '/', which is an empty final segment.
        let slug = url
            .path_segments()
            .and_then(|segments| {
                segments
                    .filter(|segment| !segment.is_empty())
                    .next_back()
                    .map(str::to_string)
            })
            .filter(|slug| !slug.contains('/'))
            .ok_or_else(|| ProviderError::Parsing("Invalid ToonWorld4All post id".into()))?;
        let mut api = Self::api_url(&requested)?;
        api.query_pairs_mut().append_pair("slug", &slug);
        if let Ok((effective_url, status, json)) = self.get_text(api.clone()).await
            && status.is_success()
            && effective_url.path() == api.path()
            && let Ok(base) = site::response_base(&effective_url)
        {
            origins.site_aliases.push(requested.clone());
            origins.site = base.clone();
            if let Ok(post) = parser::api_post(id, &origins, &json) {
                self.site_domain.accept_redirect(&requested, &base).await;
                return Ok(post);
            }
        }
        let (effective_url, status, html) = self.get_text(url).await?;
        if status == StatusCode::NOT_FOUND {
            return Err(ProviderError::NotFound);
        }
        if !status.is_success() {
            return Err(ProviderError::Unavailable(format!("HTTP status {status}")));
        }
        let base = site::returned_item_base(id, &effective_url)?;
        origins.site = base.clone();
        let post = parser::html_post(id, &origins, &html)?;
        self.site_domain.accept_redirect(&requested, &base).await;
        Ok(post)
    }

    pub async fn resolve_release(
        &self,
        release: &Release,
        intent: ResolutionIntent,
    ) -> Result<PlaybackSource, ProviderError> {
        let origins = self.origins().await;
        resolver::resolve_release(&self.client, &self.no_redirect, &origins, release, intent).await
    }
}

impl Provider for ToonWorld4AllClient {
    fn id(&self) -> ProviderKind {
        ProviderKind::ToonWorld4All
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
        let origins = self.origins().await;
        let requested = origins.site;
        let mut aliases = origins.site_aliases;
        let mut api = Self::api_url(&requested)?;
        api.query_pairs_mut()
            .append_pair("search", query.trim())
            .append_pair("per_page", "20")
            .append_pair("page", &page.max(1).to_string())
            .append_pair("_fields", "link,title,meta");
        if let Ok((effective_url, status, json)) = self.get_text(api.clone()).await {
            if status == StatusCode::BAD_REQUEST && page > 1 {
                // The WP API uses HTTP 400 for pages after the final result page.
                return Ok(Vec::new());
            }
            if status.is_success()
                && effective_url.path() == api.path()
                && let Ok(base) = site::response_base(&effective_url)
                && let Ok(items) = parser::api_search_with_aliases(&base, &aliases, &json)
            {
                if !items.is_empty() || requested.origin() == base.origin() {
                    self.site_domain.accept_redirect(&requested, &base).await;
                }
                return Ok(items);
            }
        }
        let mut url = if page > 1 {
            requested
                .join(&format!("page/{page}/"))
                .map_err(|_| ProviderError::Parsing("Invalid search page".into()))?
        } else {
            requested.clone()
        };
        url.query_pairs_mut().append_pair("s", query.trim());
        let (effective_url, status, html) = self.get_text(url).await?;
        if status == StatusCode::NOT_FOUND && page > 1 {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(ProviderError::Unavailable(format!("HTTP status {status}")));
        }
        let base = site::response_base(&effective_url)?;
        aliases.push(requested.clone());
        let items = parser::html_search_with_aliases(&base, &aliases, &html);
        if !items.is_empty() {
            self.site_domain.accept_redirect(&requested, &base).await;
        }
        Ok(items)
    }

    async fn details(&self, id: &str) -> Result<MediaDetails, ProviderError> {
        self.post(id).await.map(|(details, _)| details)
    }
}

impl ReleaseProvider for ToonWorld4AllClient {
    async fn episode_streams(
        &self,
        id: &str,
        season: usize,
        episode: usize,
    ) -> Result<Vec<Release>, ProviderError> {
        let (details, releases) = self.post(id).await?;
        let movie = season == 0 && episode == 0;
        let mut origins = self.origins().await;
        let mut selected = Vec::new();
        let mut archive_error = None;
        let mut visited = std::collections::HashSet::new();
        for release in releases.into_iter().filter(|release| {
            if movie {
                release.season.is_none()
            } else {
                release.season == Some(season) && release.episode == Some(episode)
            }
        }) {
            let mut direct = release.clone();
            direct.mirrors.clear();
            for mut mirror in release.mirrors {
                let Ok(raw_url) = site::safe_url(&mirror.resolver_url) else {
                    continue;
                };
                let url = parser::canonical_link(&origins, &raw_url);
                mirror.resolver_url = url.to_string();
                let archive = if movie {
                    parser::is_archive_movie(&origins, &url)
                } else {
                    parser::is_archive_episode(&origins, &url)
                };
                if !archive {
                    direct.mirrors.push(mirror);
                    continue;
                }
                if !visited.insert(url.to_string()) {
                    continue;
                }
                match self.get_text(url.clone()).await {
                    Ok((effective_url, status, html)) if status.is_success() => {
                        if url.path().trim_end_matches('/')
                            != effective_url.path().trim_end_matches('/')
                        {
                            archive_error = Some(ProviderError::Unavailable(
                                "Archive redirected to a different page".into(),
                            ));
                            continue;
                        }
                        let Ok(base) = site::response_base(&effective_url) else {
                            continue;
                        };
                        let mut updated = origins.clone();
                        if effective_url.origin() != origins.site.origin()
                            || url.origin() != origins.site.origin()
                        {
                            updated.archive_aliases.push(origins.archive.clone());
                            updated.archive = base.clone();
                        }
                        let files = parser::archive_releases(
                            &updated,
                            &effective_url,
                            &details.title,
                            (!movie).then_some((season, episode)),
                            &html,
                        );
                        if !files.is_empty() {
                            if updated.archive.origin() != origins.archive.origin() {
                                self.archive_domain
                                    .accept_redirect(&origins.archive, &base)
                                    .await;
                                origins = updated;
                            }
                            selected.extend(files);
                        }
                    }
                    Ok((_, status, _)) => {
                        archive_error = Some(ProviderError::Unavailable(format!(
                            "Archive file listing returned {status}"
                        )));
                    }
                    Err(error) => archive_error = Some(error),
                }
            }
            if !direct.mirrors.is_empty() {
                selected.push(direct);
            }
        }
        if selected.is_empty() {
            let reason = if movie {
                "No movie files exposed by the archive"
            } else {
                "No mirrors exposed for this episode in the archive"
            };
            return Err(archive_error.unwrap_or_else(|| ProviderError::Unavailable(reason.into())));
        }
        let mut merged: Vec<Release> = Vec::new();
        for release in selected {
            if let Some(existing) = merged.iter_mut().find(|item| {
                item.filename == release.filename
                    && item.season == release.season
                    && item.episode == release.episode
            }) {
                for mirror in release.mirrors {
                    if !existing
                        .mirrors
                        .iter()
                        .any(|item| item.resolver_url == mirror.resolver_url)
                    {
                        existing.mirrors.push(mirror);
                    }
                }
            } else {
                merged.push(release);
            }
        }
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn redirects_migrate_wordpress_and_archive_without_revisiting_old_domains() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let old_wp_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let new_wp_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let old_archive_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let new_archive_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let old_wp = format!("http://{}", old_wp_server.local_addr().unwrap());
        let new_wp = format!("http://{}", new_wp_server.local_addr().unwrap());
        let old_archive = format!("http://{}", old_archive_server.local_addr().unwrap());
        let new_archive = format!("http://{}", new_archive_server.local_addr().unwrap());
        let old_wp_hits = Arc::new(AtomicUsize::new(0));
        let old_archive_hits = Arc::new(AtomicUsize::new(0));
        let new_wp_target = new_wp.clone();
        let wp_hits = old_wp_hits.clone();
        let wp_redirect = tokio::spawn(async move {
            loop {
                let (mut stream, _) = old_wp_server.accept().await.unwrap();
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap();
                wp_hits.fetch_add(1, Ordering::SeqCst);
                let location = format!("{new_wp_target}{path}");
                let reply = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let old_wp_link = old_wp.clone();
        let old_archive_link = old_archive.clone();
        let wp_content = tokio::spawn(async move {
            loop {
                let (mut stream, _) = new_wp_server.accept().await.unwrap();
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap();
                let body = if path.starts_with("/wp-json/wp/v2/posts?search=") {
                    serde_json::json!([{"link": format!("{old_wp_link}/film/"), "title": {"rendered": "Film (2026)"}}]).to_string()
                } else if path.starts_with("/wp-json/wp/v2/posts?slug=film") {
                    serde_json::json!([{
                        "link": format!("{old_wp_link}/film/"),
                        "title": {"rendered": "Film (2026)"},
                        "content": {"rendered": format!("<a href='{old_archive_link}/movie/film'>Get Download Links</a>")}
                    }]).to_string()
                } else {
                    panic!("Unexpected WordPress request: {path}");
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let archive_hits = old_archive_hits.clone();
        let new_archive_target = new_archive.clone();
        let archive_redirect = tokio::spawn(async move {
            loop {
                let (mut stream, _) = old_archive_server.accept().await.unwrap();
                let mut buf = [0; 1024];
                let n = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap();
                archive_hits.fetch_add(1, Ordering::SeqCst);
                let location = format!("{new_archive_target}{path}");
                let reply = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let archive_content = tokio::spawn(async move {
            loop {
                let (mut stream, _) = new_archive_server.accept().await.unwrap();
                let mut buf = [0; 1024];
                let n = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap();
                assert_eq!(path, "/movie/film");
                let body = "<h3>480p SD</h3><h4>HubCloud</h4><a href='/redirect/one'>DOWNLOAD</a>";
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client =
            ToonWorld4AllClient::with_endpoints(&old_wp, &old_archive, DEFAULT_WORKER).unwrap();
        let found = Provider::search(&client, "film", 1).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.value, "/film/");
        for _ in 0..2 {
            let releases = ReleaseProvider::episode_streams(&client, "/film/", 0, 0)
                .await
                .unwrap();
            assert_eq!(releases.len(), 1);
            assert_eq!(releases[0].quality.as_deref(), Some("480p"));
            assert_eq!(
                releases[0].mirrors[0].resolver_url,
                format!("{new_archive}/redirect/one")
            );
        }
        assert_eq!(old_wp_hits.load(Ordering::SeqCst), 1);
        assert_eq!(old_archive_hits.load(Ordering::SeqCst), 1);
        for task in [wp_redirect, wp_content, archive_redirect, archive_content] {
            task.abort();
        }
    }

    #[tokio::test]
    async fn search_details_episode_and_download_use_the_selected_provider() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mock_base = base.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut request = [0; 4096];
                let Ok(n) = stream.read(&mut request).await else {
                    continue;
                };
                let first = String::from_utf8_lossy(&request[..n]);
                let path = first.split_whitespace().nth(1).unwrap_or_default();
                let (status, mime, body) = if path.starts_with("/wp-json/wp/v2/posts?search=")
                    && path.contains("&page=2")
                {
                    (
                        "400 Bad Request",
                        "application/json",
                        "{\"code\":\"rest_post_invalid_page_number\"}".into(),
                    )
                } else if path.starts_with("/wp-json/wp/v2/posts?search=") {
                    ("200 OK", "application/json", serde_json::json!([
                        {"link": format!("{mock_base}/show-season-1/"), "title": {"rendered": "Show Season 1"}},
                        {"link": format!("{mock_base}/movie/"), "title": {"rendered": "Movie (2026)"}},
                        {"link": format!("{mock_base}/toy-story-5/"), "title": {"rendered": "Toy Story 5 (2026) 480p 2160p"}}
                    ]).to_string())
                } else if path.starts_with("/wp-json/wp/v2/posts?slug=show-season-1") {
                    ("200 OK", "application/json", serde_json::json!([{
                        "link": format!("{mock_base}/show-season-1/"), "title": {"rendered": "Show Season 1"},
                        "content": {"rendered": format!("<p>Synopsis: Welcome.</p><strong>Episode 01</strong><a href='{mock_base}/episode/show-1x1'>Watch/Download</a>")}
                    }]).to_string())
                } else if path.starts_with("/wp-json/wp/v2/posts?slug=movie") {
                    ("200 OK", "application/json", serde_json::json!([{
                        "link": format!("{mock_base}/movie/"), "title": {"rendered": "Movie (2026)"},
                        "content": {"rendered": format!("<p>⇒ <strong>480p SD</strong></p><a href='{mock_base}/media.mkv'>Direct</a><p>⇒ <strong>720p HD</strong></p><a href='https://backend.tw4all.workers.dev/redirect?data=gated'>HubCloud</a>")}
                    }]).to_string())
                } else if path.starts_with("/wp-json/wp/v2/posts?slug=toy-story-5") {
                    ("200 OK", "application/json", serde_json::json!([{
                        "link": format!("{mock_base}/toy-story-5/"),
                        "title": {"rendered": "Toy Story 5 (2026) 480p 2160p"},
                        "content": {"rendered": "<p>Single Download Links</p><a href='https://archive.toonworld4all.me/movie/toy-story-5'>Get Download Links</a>"}
                    }]).to_string())
                } else if path == "/episode/show-1x1" {
                    ("200 OK", "text/html", "<h3>480p x264</h3><div>AD SYSTEM</div><h3>480p x264</h3><h4>HubCloud</h4><a href='/redirect/file'>DOWNLOAD</a>".into())
                } else if path == "/movie/toy-story-5" {
                    ("200 OK", "text/html", "<h3>480p x264</h3><h3>2160p HEVC</h3><div>AD SYSTEM</div><h3>480p x264</h3><h4>HubCloud</h4><a href='/redirect/file'>DOWNLOAD</a><h4>Filepress</h4><a href='/redirect/expired'>DOWNLOAD</a>".into())
                } else if path == "/redirect/file" {
                    (
                        "200 OK",
                        "text/html",
                        format!("<h3>Destination URL</h3><code>{mock_base}/media.mkv</code>"),
                    )
                } else if path == "/redirect/expired" {
                    ("404 Not Found", "text/html", "Expired".into())
                } else if path == "/media.mkv" {
                    ("206 Partial Content", "video/x-matroska", "x".into())
                } else if path == "/redirect?data=gated" {
                    let reply = "HTTP/1.1 302 Found\r\nLocation: https://srnky.com/ads\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    stream.write_all(reply.as_bytes()).await.unwrap();
                    continue;
                } else {
                    panic!("Unexpected mock request: {path}");
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client = ToonWorld4AllClient::with_endpoints(&base, &base, &base).unwrap();
        let results = Provider::search(&client, "show", 1).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(
            Provider::search(&client, "show", 2)
                .await
                .unwrap()
                .is_empty()
        );
        let show = Provider::details(&client, &results[0].id.value)
            .await
            .unwrap();
        assert_eq!(show.seasons[0].episodes[0].number, 1);
        let streams = ReleaseProvider::episode_streams(&client, &results[0].id.value, 1, 1)
            .await
            .unwrap();
        assert_eq!(streams[0].quality.as_deref(), Some("480p"));
        let source = client
            .resolve_release(&streams[0], ResolutionIntent::Download)
            .await
            .unwrap();
        assert_eq!(source.url, format!("{base}/media.mkv"));
        assert_eq!(source.provider, ProviderKind::ToonWorld4All);
        let movie = ReleaseProvider::episode_streams(&client, &results[1].id.value, 0, 0)
            .await
            .unwrap();
        assert_eq!(movie.len(), 2);
        assert!(
            movie
                .iter()
                .flat_map(|release| &release.mirrors)
                .all(|mirror| mirror.resolver_url.starts_with(&base))
        );
        let movie_media = client
            .resolve_release(&movie[0], ResolutionIntent::Playback)
            .await
            .unwrap();
        assert_eq!(movie_media.url, format!("{base}/media.mkv"));
        assert!(
            client
                .resolve_release(&movie[1], ResolutionIntent::Playback)
                .await
                .is_err()
        );
        let archive_movie = Provider::details(&client, &results[2].id.value)
            .await
            .unwrap();
        assert!(!archive_movie.is_series());
        let archive_files = ReleaseProvider::episode_streams(&client, &results[2].id.value, 0, 0)
            .await
            .unwrap();
        assert_eq!(archive_files.len(), 1);
        assert_eq!(archive_files[0].quality.as_deref(), Some("480p"));
        assert_eq!(archive_files[0].mirrors.len(), 2);
        assert!(archive_files[0].season.is_none());
        let movie_source = client
            .resolve_release(&archive_files[0], ResolutionIntent::Download)
            .await
            .unwrap();
        assert_eq!(movie_source.url, format!("{base}/media.mkv"));
        server.abort();
    }
}
