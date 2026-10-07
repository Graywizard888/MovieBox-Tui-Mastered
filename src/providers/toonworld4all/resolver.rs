//! Resolve ToonWorld4All archive/worker wrappers before trying the existing media preflight.
//! Ad shorteners and HTML landing pages are deliberately never returned to playback/download.
use reqwest::Url;
use reqwest::header::{CONTENT_TYPE, COOKIE, LOCATION};
use scraper::{Html, Selector};
use std::collections::HashSet;

use super::super::models::{
    PlaybackSource, ProviderError, ProviderKind, Release, ResolutionIntent, SourceMirror,
};
use super::super::{drive, site};
use super::parser::{self, Origins};

pub(super) fn supported_host(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let parts: Vec<&str> = host.split('.').collect();
    let file_host = parts.len() >= 2
        && matches!(
            parts[parts.len() - 2],
            "hubcloud" | "hubdrive" | "filepress" | "gdflix" | "appdrive" | "pixeldrain" | "gofile"
        )
        && matches!(
            parts[parts.len() - 1],
            "ist"
                | "baby"
                | "lat"
                | "dev"
                | "io"
                | "com"
                | "net"
                | "xyz"
                | "in"
                | "org"
                | "lol"
                | "site"
                | "app"
                | "foo"
                | "wiki"
                | "life"
                | "me"
        );
    file_host
        || matches!(
            host.as_str(),
            "mega.nz" | "drive.google.com" | "docs.google.com"
        )
}

fn is_wrapper(url: &Url, origins: &Origins) -> bool {
    (url.origin() == origins.archive.origin() && url.path().starts_with("/redirect/"))
        || (url.origin() == origins.worker.origin() && url.path() == "/redirect")
        || (url.origin() == origins.site.origin() && url.path().starts_with("/redirect/"))
}

fn allowed_target(url: &Url, origins: &Origins) -> bool {
    drive::is_media_url(url)
        || drive::is_drive_page(url)
        || supported_host(url)
        || is_wrapper(url, origins)
}

fn shortener_error() -> ProviderError {
    ProviderError::Unavailable(
        "Mirror requires an interactive ad shortener; solve it once in a browser and set \
         MOVIEBOX_TOONWORLD_COOKIE to that browser's archive.toonworld4all.me cookies, or \
         choose another mirror or quality"
            .into(),
    )
}

/// Cookies copied from a browser that already passed the archive's 24-hour ad gate. They are
/// supplied by the user, sent only to the archive's own redirect pages, and never logged.
fn archive_cookie() -> Option<String> {
    std::env::var("MOVIEBOX_TOONWORLD_COOKIE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
}

async fn limited_html(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<String, ProviderError> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(ProviderError::Parsing("Redirect page is too large".into()));
    }
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(site::network_error)? {
        if data.len() + chunk.len() > limit {
            return Err(ProviderError::Parsing("Redirect page is too large".into()));
        }
        data.extend_from_slice(&chunk);
    }
    let html = String::from_utf8_lossy(&data).into_owned();
    site::check_page(&html)?;
    Ok(html)
}

async fn unwrap_redirect(
    client: &reqwest::Client,
    url: &Url,
    archive: bool,
    cookie: Option<&str>,
    gate_seen: &mut bool,
) -> Result<Url, ProviderError> {
    site::safe_url(url.as_str())?;
    let mut request = client.get(url.clone());
    if archive && let Some(cookie) = cookie {
        request = request.header(COOKIE, cookie);
    }
    let response = request.send().await.map_err(site::network_error)?;
    let status = response.status();
    if status.is_redirection() {
        let target = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|location| site::external_url(url, location))
            .ok_or_else(|| ProviderError::Parsing("Redirect destination missing".into()))?;
        return Ok(target);
    }
    if !status.is_success() {
        return Err(ProviderError::Unavailable("Mirror link has expired".into()));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.starts_with("video/") || content_type.contains("octet-stream") {
        return Ok(url.clone());
    }
    let html = limited_html(response, 512 * 1024).await?;
    if archive {
        // The page that asks the visitor to pick an ad shortener shows a placeholder
        // destination that is not a real file until the gate is passed.
        *gate_seen |= html.contains("Shortener Redirect System");
        parser::destination_url(&html).ok_or_else(|| {
            ProviderError::Unavailable("Archive destination is hidden behind an ad gate".into())
        })
    } else {
        // Worker redirect tokens are encrypted, not base64-encoded media URLs. If no explicit
        // destination is visible, do not guess or pass the worker's HTML to the media player.
        parser::destination_url(&html).ok_or_else(shortener_error)
    }
}

fn pixeldrain_media(url: &Url) -> Option<Url> {
    let host = url.host_str()?;
    if !host.split('.').any(|part| part == "pixeldrain") {
        return None;
    }
    let id = url.path().strip_prefix("/u/")?.trim_matches('/');
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    let mut result = url.clone();
    result.set_path(&format!("/api/file/{id}"));
    result.set_query(Some("download"));
    Some(result)
}

fn download_links(base: &Url, html: &str) -> Vec<Url> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").unwrap();
    let mut seen = HashSet::new();
    document
        .select(&selector)
        .filter_map(|link| {
            let label = site::text(Some(link))
                .unwrap_or_default()
                .to_ascii_lowercase();
            let href = link.value().attr("href")?;
            let target = site::external_url(base, href)?;
            let is_download = link.value().attr("id") == Some("download")
                || link.value().attr("class").is_some_and(|class| {
                    class.contains("btn-success") || class.contains("btn-primary")
                })
                || label.contains("download")
                || label.contains("pixeldrain")
                || drive::is_media_url(&target);
            (is_download
                && target != *base
                && (supported_host(&target) || drive::is_media_url(&target)))
            .then_some(target)
        })
        .filter(|url| seen.insert(url.to_string()))
        .take(12)
        .collect()
}

async fn media(
    client: &reqwest::Client,
    release: &Release,
    mirror: &SourceMirror,
    url: &Url,
    intent: ResolutionIntent,
    referer: Option<&Url>,
) -> Result<PlaybackSource, ProviderError> {
    let mut candidate = release.clone();
    candidate.mirrors = vec![mirror.clone()];
    candidate.mirrors[0].resolver_url = url.to_string();
    if let Some(referer) = referer {
        candidate.mirrors[0]
            .headers
            .push(("Referer".into(), referer.as_str().into()));
    }
    drive::resolve_release(client, &candidate, ProviderKind::ToonWorld4All, intent).await
}

async fn resolve_landing(
    client: &reqwest::Client,
    release: &Release,
    mirror: &SourceMirror,
    url: &Url,
    intent: ResolutionIntent,
) -> Result<PlaybackSource, ProviderError> {
    if let Some(api) = pixeldrain_media(url) {
        return media(client, release, mirror, &api, intent, None).await;
    }
    if drive::is_media_url(url) || drive::is_drive_page(url) {
        return media(client, release, mirror, url, intent, None).await;
    }
    // HubCloud's /drive/ page first links to a resolver page, which then lists video/CDN
    // buttons. Other file hosts sometimes list the direct download on their first page.
    // Restrict candidate links to known file hosts or media URLs; never follow ad buttons.
    if let Ok(page) = drive::link_page(client, url).await {
        if page.html.is_none() {
            return media(client, release, mirror, &page.url, intent, None).await;
        }
        if supported_host(&page.url) || drive::is_media_url(&page.url) {
            for next in download_links(&page.url, page.html.as_deref().unwrap_or_default()) {
                if drive::is_media_url(&next) || pixeldrain_media(&next).is_some() {
                    let next = pixeldrain_media(&next).unwrap_or(next);
                    if let Ok(source) =
                        media(client, release, mirror, &next, intent, Some(&page.url)).await
                    {
                        return Ok(source);
                    }
                } else if let Ok(inner) = drive::link_page(client, &next).await {
                    if let Some(html) = inner.html {
                        for candidate in download_links(&inner.url, &html) {
                            let candidate = pixeldrain_media(&candidate).unwrap_or(candidate);
                            if let Ok(source) = media(
                                client,
                                release,
                                mirror,
                                &candidate,
                                intent,
                                Some(&inner.url),
                            )
                            .await
                            {
                                return Ok(source);
                            }
                        }
                    } else if let Ok(source) =
                        media(client, release, mirror, &inner.url, intent, None).await
                    {
                        return Ok(source);
                    }
                }
            }
        }
    }
    // Reuse the Driveleech/Driveseed resolver for landing formats which it understands.
    media(client, release, mirror, url, intent, None).await
}

pub(super) async fn resolve_release(
    client: &reqwest::Client,
    no_redirect: &reqwest::Client,
    origins: &Origins,
    release: &Release,
    intent: ResolutionIntent,
) -> Result<PlaybackSource, ProviderError> {
    if release.provider != ProviderKind::ToonWorld4All {
        return Err(ProviderError::Parsing(
            "Release belongs to another provider".into(),
        ));
    }
    let mut ad_gate = false;
    let mut gate_seen = false;
    let cookie = archive_cookie();
    for mirror in release.mirrors.iter().take(8) {
        let result = async {
            let mut target =
                parser::canonical_link(origins, &site::safe_url(&mirror.resolver_url)?);
            let mut visited = HashSet::new();
            for _ in 0..3 {
                if !visited.insert(target.to_string()) {
                    break;
                }
                let archive = target.origin() == origins.archive.origin()
                    && target.path().starts_with("/redirect/");
                if is_wrapper(&target, origins) {
                    target = parser::canonical_link(
                        origins,
                        &unwrap_redirect(
                            no_redirect,
                            &target,
                            archive,
                            cookie.as_deref(),
                            &mut gate_seen,
                        )
                        .await?,
                    );
                    if !allowed_target(&target, origins) {
                        return Err(shortener_error());
                    }
                } else {
                    if !allowed_target(&target, origins) {
                        return Err(shortener_error());
                    }
                    return resolve_landing(client, release, mirror, &target, intent).await;
                }
            }
            Err(ProviderError::Unavailable(
                "Too many mirror redirects".into(),
            ))
        }
        .await;
        match result {
            Ok(source) => return Ok(source),
            Err(error) => {
                if let ProviderError::Unavailable(message) = &error
                    && (message.contains("shortener") || message.contains("ad gate"))
                {
                    ad_gate = true;
                }
                log::debug!("ToonWorld4All mirror resolution failed: {error}");
            }
        }
    }
    Err(if ad_gate || gate_seen {
        shortener_error()
    } else {
        ProviderError::Unavailable(
            "No working direct media link; the selected mirrors may have expired".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn archive_and_worker_redirects_only_yield_verified_media() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let local_base = base.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut request = [0; 2048];
                let Ok(n) = stream.read(&mut request).await else {
                    continue;
                };
                let line = String::from_utf8_lossy(&request[..n]);
                let path = line.split_whitespace().nth(1).unwrap_or_default();
                let (status, headers, body) = match path {
                    "/redirect/one" => (
                        "200 OK",
                        "Content-Type: text/html\r\n".to_string(),
                        format!("<h2>Destination URL</h2><code>{local_base}/media.mkv</code>"),
                    ),
                    "/redirect?data=good" => (
                        "302 Found",
                        format!("Location: {local_base}/media.mkv\r\n"),
                        String::new(),
                    ),
                    "/redirect?data=gated" => (
                        "302 Found",
                        "Location: https://srnky.com/ad\r\n".into(),
                        String::new(),
                    ),
                    "/redirect?data=html" => (
                        "200 OK",
                        "Content-Type: text/html\r\n".into(),
                        "<p>watch an ad to proceed</p>".into(),
                    ),
                    "/media.mkv" => (
                        "206 Partial Content",
                        "Content-Type: video/x-matroska\r\n".into(),
                        "x".into(),
                    ),
                    "/landing.mkv" => (
                        "200 OK",
                        "Content-Type: text/html\r\n".into(),
                        "<p>not a movie</p>".into(),
                    ),
                    _ => panic!("Unexpected request: {path}"),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let url = Url::parse(&base).unwrap();
        let origins = Origins {
            site: url.clone(),
            archive: url.clone(),
            worker: url,
            site_aliases: vec![],
            archive_aliases: vec![],
            worker_aliases: vec![],
        };
        let client = site::browser_client().unwrap();
        let no_redirect = crate::net::http_client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let release = |path: &str| Release {
            provider: ProviderKind::ToonWorld4All,
            filename: "Demo 480p".into(),
            quality: Some("480p".into()),
            codec: None,
            language: None,
            size_bytes: None,
            season: None,
            episode: None,
            resource_id: None,
            mirrors: vec![SourceMirror {
                label: "HubCloud".into(),
                resolver_url: format!("{base}{path}"),
                direct_file: false,
                headers: Vec::new(),
            }],
        };
        for path in ["/redirect/one", "/redirect?data=good"] {
            let found = resolve_release(
                &client,
                &no_redirect,
                &origins,
                &release(path),
                ResolutionIntent::Playback,
            )
            .await
            .unwrap();
            assert_eq!(found.url, format!("{base}/media.mkv"));
            assert_eq!(found.provider, ProviderKind::ToonWorld4All);
        }
        for path in ["/redirect?data=gated", "/redirect?data=html"] {
            let error = resolve_release(
                &client,
                &no_redirect,
                &origins,
                &release(path),
                ResolutionIntent::Download,
            )
            .await
            .unwrap_err();
            assert!(
                error
                    .user_message(ProviderKind::ToonWorld4All)
                    .contains("shortener")
            );
        }
        let error = resolve_release(
            &client,
            &no_redirect,
            &origins,
            &release("/landing.mkv"),
            ResolutionIntent::Playback,
        )
        .await
        .unwrap_err();
        assert!(!error.to_string().is_empty());
        let good = resolve_release(
            &client,
            &no_redirect,
            &origins,
            &release("/media.mkv"),
            ResolutionIntent::Download,
        )
        .await
        .unwrap();
        assert_eq!(good.url, format!("{base}/media.mkv"));
        server.abort();
    }

    #[test]
    fn rejects_shortener_as_supported_host() {
        for host in ["srnky.com", "ads.hubcloud.ist.bad.example", "localhost"] {
            assert!(!supported_host(
                &Url::parse(&format!("https://{host}/file")).unwrap()
            ));
        }
        assert!(supported_host(
            &Url::parse("https://new4.filepress.baby/file/id").unwrap()
        ));
        assert!(supported_host(
            &Url::parse("https://new5.filepress.wiki/file/id").unwrap()
        ));
        assert!(supported_host(
            &Url::parse("https://hubcloud.foo/drive/id").unwrap()
        ));
    }
}
