//! Resolves provider link pages to actual media URLs. Never pass a landing page or a
//! Driveleech HTML page to the player or downloader.
use std::collections::{HashMap, HashSet, VecDeque};

use base64::Engine;
use reqwest::Url;
use reqwest::header::{CONTENT_TYPE, COOKIE, RANGE, REFERER, SET_COOKIE};
use scraper::{Html, Selector};

use super::models::{
    PlaybackSource, ProviderError, ProviderKind, Release, ResolutionIntent, SourceMirror,
};
use super::site;

const MAX_HTML_BYTES: usize = 1024 * 1024;
const MAX_STEPS: usize = 24;

pub(super) fn is_media_url(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    [".mkv", ".mp4", ".webm", ".avi", ".ts", ".m3u8", ".mpd"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

pub(super) fn is_drive_page(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|host| host.contains("driveseed.") || host.contains("driveleech."))
        && (url.path().starts_with("/file/") || url.path().starts_with("/r"))
}

fn is_video_seed(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|host| host.contains("video-seed.") || host.contains("video-leech."))
}

/// Link pages sometimes wrap the destination as base64 `?url=...` (Moviesmod) or as
/// percent-encoded `?url=...` (video-seed). Only return a real, safe URL, never a token.
pub(super) fn unwrap_url(url: &Url) -> Option<Url> {
    let encoded = url.query_pairs().find(|(key, _)| key == "url")?.1;
    if let Ok(target) = site::safe_url(&encoded) {
        return Some(target);
    }
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(bytes) = engine.decode(encoded.as_bytes())
            && let Ok(decoded) = String::from_utf8(bytes)
            && let Ok(target) = site::safe_url(decoded.trim())
        {
            return Some(target);
        }
    }
    None
}

#[derive(Default)]
struct Cookies(HashMap<String, String>);

impl Cookies {
    fn absorb(&mut self, response: &reqwest::Response) {
        for value in response.headers().get_all(SET_COOKIE).iter() {
            if let Ok(pair) = value.to_str()
                && let Some((key, val)) = pair.split(';').next().and_then(|s| s.split_once('='))
                && !key.trim().is_empty()
            {
                self.0.insert(key.trim().into(), val.trim().into());
            }
        }
    }

    fn header(&self) -> String {
        self.0
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

pub(super) struct Page {
    pub(super) url: Url,
    pub(super) html: Option<String>,
}

pub(super) async fn link_page(client: &reqwest::Client, url: &Url) -> Result<Page, ProviderError> {
    fetch_page(client, url, None, None).await
}

async fn fetch_page(
    client: &reqwest::Client,
    url: &Url,
    cookies: Option<&mut Cookies>,
    referer: Option<&Url>,
) -> Result<Page, ProviderError> {
    fetch_page_checked(client, url, cookies, referer, true).await
}

/// `strict` rejects "not a robot" interstitials; gate hops pass `false` (see `check_challenge`).
async fn fetch_page_checked(
    client: &reqwest::Client,
    url: &Url,
    cookies: Option<&mut Cookies>,
    referer: Option<&Url>,
    strict: bool,
) -> Result<Page, ProviderError> {
    site::safe_url(url.as_str())?;
    let mut request = client.get(url.clone());
    if let Some(referer) = referer {
        request = request.header(REFERER, referer.as_str());
    }
    if let Some(ref jar) = cookies
        && !jar.0.is_empty()
    {
        request = request.header(COOKIE, jar.header());
    }
    let mut response = request
        .send()
        .await
        .map_err(site::network_error)?
        .error_for_status()
        .map_err(site::network_error)?;
    if let Some(jar) = cookies {
        jar.absorb(&response);
    }
    let final_url = site::safe_url(response.url().as_str())?;
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.starts_with("video/") || content_type.contains("octet-stream") {
        return Ok(Page {
            url: final_url,
            html: None,
        });
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_HTML_BYTES as u64)
    {
        return Err(ProviderError::Parsing("Link page is too large".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(site::network_error)? {
        if body.len() + chunk.len() > MAX_HTML_BYTES {
            return Err(ProviderError::Parsing("Link page is too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    let html = String::from_utf8_lossy(&body).into_owned();
    if strict {
        site::check_page(&html)?;
    } else {
        site::check_challenge(&html)?;
    }
    Ok(Page {
        url: final_url,
        html: Some(html),
    })
}

async fn post_form(
    client: &reqwest::Client,
    url: &Url,
    data: &[(String, String)],
    referer: &Url,
    jar: &mut Cookies,
) -> Result<Page, ProviderError> {
    site::safe_url(url.as_str())?;
    let mut req = client
        .post(url.clone())
        .header(REFERER, referer.as_str())
        .form(data);
    if !jar.0.is_empty() {
        req = req.header(COOKIE, jar.header());
    }
    let mut response = req
        .send()
        .await
        .map_err(site::network_error)?
        .error_for_status()
        .map_err(site::network_error)?;
    jar.absorb(&response);
    let final_url = site::safe_url(response.url().as_str())?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(site::network_error)? {
        if body.len() + chunk.len() > MAX_HTML_BYTES {
            return Err(ProviderError::Parsing("Link page is too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    let html = String::from_utf8_lossy(&body).into_owned();
    site::check_challenge(&html)?;
    Ok(Page {
        url: final_url,
        html: Some(html),
    })
}

/// Gate pages hide the next hop in an auto-submitted POST form. Older gates used
/// `form#landing` with `_wp_*` fields; the current LinkPilot gate uses `lp-*` form ids with
/// `_lp_*` fields. Recognize both by id or by their marker fields, never by position.
fn landing_form(url: &Url, html: &str) -> Option<(Url, Vec<(String, String)>)> {
    let document = Html::parse_document(html);
    let forms = Selector::parse("form[action]").ok()?;
    let fields = Selector::parse("input[name]").ok()?;
    for form in document.select(&forms) {
        let id = form.value().attr("id").unwrap_or_default();
        let inputs = form
            .select(&fields)
            .map(|input| {
                (
                    input.value().attr("name").unwrap_or_default().to_string(),
                    input.value().attr("value").unwrap_or_default().to_string(),
                )
            })
            .filter(|(name, _)| !name.is_empty())
            .collect::<Vec<_>>();
        let gated = id == "landing"
            || id.starts_with("lp-")
            || inputs
                .iter()
                .any(|(name, _)| name.starts_with("_lp_") || name.starts_with("_wp_"));
        if !gated || inputs.is_empty() {
            continue;
        }
        let action = site::external_url(url, form.value().attr("action")?)?;
        // The form may be on a fake blog path, but must remain on the landing host.
        if action.origin() != url.origin() {
            continue;
        }
        return Some((action, inputs));
    }
    None
}

/// The final LinkPilot hop stores a one-time cookie from inline script (`sc("lp-…","<value>",60)`)
/// and exposes the next URL as `/?lp_go=<cookie name>`. Replay exactly that pair.
fn lp_go(base: &Url, html: &str) -> Option<(Url, String, String)> {
    let tail = html.split("?lp_go=").nth(1)?;
    let name: String = tail
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .take(120)
        .collect();
    if name.is_empty() {
        return None;
    }
    let marker = format!("\"{name}\",\"");
    let value_tail = html.split(&marker).nth(1)?;
    let value: String = value_tail
        .split('"')
        .next()?
        .replace("\\/", "/")
        .chars()
        .take(4096)
        .collect();
    if value.is_empty() {
        return None;
    }
    let mut go = base.clone();
    go.set_path("/");
    go.set_query(None);
    go.query_pairs_mut().append_pair("lp_go", &name);
    Some((go, name, value))
}

fn go_token(html: &str) -> Option<String> {
    let tail = html.split("?go=").nth(1)?;
    let token: String = tail
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .take(120)
        .collect();
    (!token.is_empty()).then_some(token)
}

fn refresh_url(base: &Url, html: &str) -> Option<Url> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("meta[http-equiv]").ok()?;
    let content = document
        .select(&selector)
        .find(|node| {
            node.value()
                .attr("http-equiv")
                .is_some_and(|value| value.eq_ignore_ascii_case("refresh"))
        })?
        .value()
        .attr("content")?;
    let idx = content.to_ascii_lowercase().find("url=")?;
    let target = content[idx + 4..].trim().trim_matches(['\'', '"']);
    site::external_url(base, target)
}

fn script_redirect(base: &Url, html: &str) -> Option<Url> {
    for prefix in ["location.replace(", "location.href =", "location.href="] {
        if let Some(after) = html.split(prefix).nth(1) {
            let after = after.trim_start();
            let after = after
                .strip_prefix('"')
                .or_else(|| after.strip_prefix('\''))?;
            let target = after.split(['"', '\'']).next()?;
            // Inline script written through JSON escapes slashes (`https:\/\/host\/path`).
            let target = target.replace("\\/", "/");
            if !target.starts_with("/404") {
                return site::external_url(base, &target);
            }
        }
    }
    None
}

const MAX_GATE_HOPS: usize = 8;

/// Walk a link gate (`?sid=` landing pages) to the file host it protects. Supports the old
/// two-form + `?go=` token chain and the current LinkPilot chain (`_lp_*` forms, then a
/// cookie + `?lp_go=` request). Timers on these pages are client-side only, so nothing sleeps.
async fn bypass_cloud(client: &reqwest::Client, sid: &Url) -> Result<Url, ProviderError> {
    let mut jar = Cookies::default();
    let mut page = fetch_page_checked(client, sid, Some(&mut jar), None, false).await?;
    let mut old_cookie: Option<String> = None;
    let mut posted = HashSet::new();
    for _ in 0..MAX_GATE_HOPS {
        let html = page.html.clone().unwrap_or_default();
        if let Some((action, data)) = landing_form(&page.url, &html) {
            let key = format!("{action}|{data:?}");
            if !posted.insert(key) {
                return Err(ProviderError::Parsing("Cloud gate looped".into()));
            }
            if let Some((_, value)) = data.iter().find(|(name, _)| name == "_wp_http2") {
                old_cookie = Some(value.clone());
            }
            page = post_form(client, &action, &data, &page.url, &mut jar).await?;
            continue;
        }
        if let Some((go, name, value)) = lp_go(&page.url, &html) {
            jar.0.insert(name, value);
            page = fetch_page_checked(client, &go, Some(&mut jar), Some(&page.url), false).await?;
            continue;
        }
        if let (Some(token), Some(cookie)) = (go_token(&html), old_cookie.take()) {
            jar.0.insert(token.clone(), cookie);
            let mut go = sid.clone();
            go.set_path("/");
            go.set_query(None);
            go.query_pairs_mut().append_pair("go", &token);
            let fourth =
                fetch_page_checked(client, &go, Some(&mut jar), Some(&page.url), false).await?;
            let target = refresh_url(&fourth.url, fourth.html.as_deref().unwrap_or_default())
                .ok_or_else(|| ProviderError::Parsing("Cloud redirect missing".into()))?;
            // Never send the landing cookies to the destination host.
            let fifth = fetch_page_checked(client, &target, None, Some(&go), false).await?;
            return Ok(
                script_redirect(&fifth.url, fifth.html.as_deref().unwrap_or_default())
                    .unwrap_or(fifth.url),
            );
        }
        if let Some(target) =
            script_redirect(&page.url, &html).or_else(|| refresh_url(&page.url, &html))
            && target.origin() != page.url.origin()
        {
            return Ok(target);
        }
        break;
    }
    // No known next hop. An interactive "not a robot" page is a verification gate; anything
    // else means the gate changed shape again.
    site::check_page(page.html.as_deref().unwrap_or_default())?;
    Err(ProviderError::Parsing(
        "Cloud gate format is not recognized".into(),
    ))
}

/// Extract owned labels and URLs before the resolver awaits network requests. `scraper::Html`
/// is not Send and must not remain live across an `.await` in a spawned playback task.
fn drive_buttons(base: &Url, html: &str) -> Vec<(String, Url)> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("div.text-center > a[href]").unwrap();
    document
        .select(&selector)
        .filter_map(|node| {
            let url = site::external_url(base, node.value().attr("href")?)?;
            let text = node.text().collect::<String>().to_ascii_lowercase();
            Some((text, url))
        })
        .collect()
}

fn links_with_selector(base: &Url, html: &str, selector: &str) -> Vec<Url> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse(selector) else {
        return vec![];
    };
    document
        .select(&selector)
        .filter_map(|node| node.value().attr("href"))
        .filter_map(|href| site::external_url(base, href))
        .collect()
}

async fn instant_link(client: &reqwest::Client, url: &Url) -> Vec<Url> {
    let Ok(page) = fetch_page(client, url, None, None).await else {
        return vec![];
    };
    let final_url = page.url;
    if is_video_seed(&final_url) {
        if let Some(keys) = final_url
            .query_pairs()
            .find(|(key, _)| key == "url")
            .map(|(_, val)| val.into_owned())
        {
            let host = final_url.host_str().unwrap_or_default();
            let api = final_url.join("/api").ok();
            if let Some(api) = api
                && let Ok(resp) = client
                    .post(api)
                    .header("x-token", host)
                    .header(REFERER, final_url.as_str())
                    .form(&[("keys", keys)])
                    .send()
                    .await
                && let Ok(json) = resp.json::<serde_json::Value>().await
                && let Some(direct) = json.get("url").and_then(|url| url.as_str())
                && let Ok(link) = site::safe_url(direct)
            {
                return vec![link];
            }
        }
    }
    unwrap_url(&final_url)
        .or_else(|| is_media_url(&final_url).then_some(final_url))
        .into_iter()
        .collect()
}

async fn resume_cloud(client: &reqwest::Client, url: &Url) -> Vec<Url> {
    let Ok(page) = fetch_page(client, url, None, None).await else {
        return vec![];
    };
    let Some(html) = page.html else {
        return vec![page.url];
    };
    let mut links = links_with_selector(
        &page.url,
        &html,
        "a.btn-success[href], a[href*='workers.dev']",
    );
    if !links.is_empty() {
        return links;
    }
    // Some versions mint a token URL through a zfile POST rather than showing the worker link.
    let Some(key) = quoted_argument(&html, "key") else {
        return vec![];
    };
    let data = [
        ("action", "cloud"),
        ("key", key.as_str()),
        ("action_token", ""),
    ];
    let Ok(resp) = client
        .post(page.url.clone())
        .header("x-token", page.url.origin().ascii_serialization())
        .header("X-Requested-With", "XMLHttpRequest")
        .header(REFERER, page.url.as_str())
        .form(&data)
        .send()
        .await
    else {
        return vec![];
    };
    if let Ok(json) = resp.json::<serde_json::Value>().await
        && let Some(href) = json.get("url").and_then(|url| url.as_str())
        && let Some(next) = site::external_url(&page.url, href)
        && let Ok(token_page) = fetch_page(client, &next, None, Some(&page.url)).await
        && let Some(html) = token_page.html
    {
        links.extend(links_with_selector(
            &token_page.url,
            &html,
            "a.btn-success[href]",
        ));
    }
    links
}

fn quoted_argument(html: &str, name: &str) -> Option<String> {
    for prefix in [format!("append('{name}',"), format!("append(\"{name}\",")] {
        if let Some(tail) = html.split(prefix.as_str()).nth(1) {
            let tail = tail.trim_start();
            let quote = tail.chars().next()?;
            if quote == '\'' || quote == '"' {
                return tail[1..].split(quote).next().map(str::to_string);
            }
        }
    }
    None
}

async fn resume_bot(client: &reqwest::Client, url: &Url) -> Vec<Url> {
    let mut jar = Cookies::default();
    let Ok(page) = fetch_page(client, url, Some(&mut jar), None).await else {
        return vec![];
    };
    let Some(html) = page.html else { return vec![] };
    let Some(token) = quoted_argument(&html, "token") else {
        return vec![];
    };
    let Some(id) = html
        .split("/download?id=")
        .nth(1)
        .map(|tail| {
            tail.chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '+' | '='))
                .take(256)
                .collect::<String>()
        })
        .filter(|id| !id.is_empty())
    else {
        return vec![];
    };
    let Ok(mut endpoint) = page.url.join("/download") else {
        return vec![];
    };
    endpoint.query_pairs_mut().append_pair("id", &id);
    let mut request = client
        .post(endpoint)
        .header(REFERER, page.url.as_str())
        .header("Origin", page.url.origin().ascii_serialization())
        .form(&[("token", token)]);
    if !jar.0.is_empty() {
        request = request.header(COOKIE, jar.header());
    }
    let Ok(response) = request.send().await else {
        return vec![];
    };
    let Ok(json) = response.json::<serde_json::Value>().await else {
        return vec![];
    };
    json.get("url")
        .and_then(|value| value.as_str())
        .and_then(|href| site::external_url(&page.url, href))
        .into_iter()
        .collect()
}

async fn direct_links(client: &reqwest::Client, url: &Url) -> Vec<Url> {
    let mut links = Vec::new();
    for kind in ["1", "2"] {
        let mut target = url.clone();
        target.query_pairs_mut().append_pair("type", kind);
        if let Ok(page) = fetch_page(client, &target, None, None).await
            && let Some(html) = page.html
        {
            links.extend(links_with_selector(&page.url, &html, "a.btn-success[href]"));
        }
    }
    links
}

async fn preflight(
    client: &reqwest::Client,
    url: &Url,
    mirror: &SourceMirror,
    intent: ResolutionIntent,
) -> Result<Url, ProviderError> {
    site::safe_url(url.as_str())?;
    if matches!(intent, ResolutionIntent::Download)
        && (url.path().ends_with(".m3u8") || url.path().ends_with(".mpd"))
    {
        return Err(ProviderError::Unavailable(
            "Not a downloadable media file".into(),
        ));
    }
    // Ask for a short prefix so ambiguous content types can be checked without loading
    // a full file. Some hosts serve verification HTML at URLs ending in `.mkv`.
    let mut request = client.get(url.clone()).header(RANGE, "bytes=0-511");
    for (name, value) in &mirror.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let mut response = request
        .send()
        .await
        .map_err(site::network_error)?
        .error_for_status()
        .map_err(site::network_error)?;
    let final_url = site::safe_url(response.url().as_str())?;
    if final_url.path().to_ascii_lowercase().ends_with(".zip") {
        return Err(ProviderError::Unavailable(
            "Not a playable media file".into(),
        ));
    }
    if matches!(intent, ResolutionIntent::Download)
        && (final_url.path().ends_with(".m3u8") || final_url.path().ends_with(".mpd"))
    {
        return Err(ProviderError::Unavailable(
            "Not a downloadable media file".into(),
        ));
    }
    let mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(intent, ResolutionIntent::Download) && mime.contains("mpegurl") {
        return Err(ProviderError::Unavailable(
            "Not a downloadable media file".into(),
        ));
    }
    if mime.contains("text/html")
        || mime.contains("text/plain")
        || mime.contains("application/json")
        || mime.contains("application/zip")
        || mime.contains("xml") && !final_url.path().ends_with(".mpd")
    {
        return Err(ProviderError::Unavailable(
            "Mirror returned a web page, not video".into(),
        ));
    }
    // A media-looking URL or generic download MIME is not enough: some expired hosts
    // return an HTML/JSON verification page under the same URL and content type.
    if !mime.starts_with("video/") && !mime.contains("mpegurl") {
        let prefix = response
            .chunk()
            .await
            .map_err(site::network_error)?
            .ok_or_else(|| ProviderError::Unavailable("Mirror returned no media data".into()))?;
        let preview = String::from_utf8_lossy(&prefix[..prefix.len().min(512)]);
        site::check_page(&preview)?;
        let start = preview
            .trim_start_matches('\u{feff}')
            .trim_start()
            .to_ascii_lowercase();
        let dash_manifest = final_url.path().to_ascii_lowercase().ends_with(".mpd")
            && (start.starts_with("<?xml") || start.starts_with("<mpd") || start == "<");
        if (start.starts_with('<') && !dash_manifest)
            || start.starts_with('{')
            || start.starts_with('[')
        {
            return Err(ProviderError::Unavailable(
                "Mirror returned a web page, not video".into(),
            ));
        }
    }
    if !mime.starts_with("video/")
        && !mime.contains("octet-stream")
        && !mime.contains("mpegurl")
        && !is_media_url(&final_url)
        && !response
            .headers()
            .contains_key(reqwest::header::CONTENT_DISPOSITION)
    {
        return Err(ProviderError::Unavailable(
            "Mirror is not a media file".into(),
        ));
    }
    Ok(final_url)
}

pub(super) async fn resolve_release(
    client: &reqwest::Client,
    release: &Release,
    provider: ProviderKind,
    intent: ResolutionIntent,
) -> Result<PlaybackSource, ProviderError> {
    if release.provider != provider {
        return Err(ProviderError::Parsing(
            "Release belongs to another provider".into(),
        ));
    }
    let mut verification_error = None;
    for mirror in release.mirrors.iter().take(8) {
        match resolve_mirror(client, mirror, intent).await {
            Ok((url, label)) => {
                let mut headers = mirror.headers.clone();
                if !headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                {
                    headers.push((
                        "User-Agent".into(),
                        crate::net::DEFAULT_BROWSER_USER_AGENT.into(),
                    ));
                }
                return Ok(PlaybackSource {
                    provider,
                    url: url.to_string(),
                    headers,
                    subtitle: None,
                    source_label: format!("{} • {label}", provider.label()),
                    max_height: release.quality.as_ref().map(|_| release.resolution_u64()),
                });
            }
            Err(error) => {
                log::debug!("{} mirror resolution failed: {error}", provider.label());
                if site::is_browser_verification(&error) {
                    verification_error = Some(error);
                }
            }
        }
    }
    Err(verification_error.unwrap_or_else(|| {
        ProviderError::Unavailable(
            "No working direct media link for this release. Try another quality or refresh.".into(),
        )
    }))
}

async fn resolve_mirror(
    client: &reqwest::Client,
    mirror: &SourceMirror,
    intent: ResolutionIntent,
) -> Result<(Url, String), ProviderError> {
    let start = site::safe_url(&mirror.resolver_url)?;
    let mut pending = VecDeque::from([(start, 0_usize, mirror.label.clone())]);
    let mut visited = HashSet::new();
    let mut steps = 0;
    let mut verification_error = None;
    while let Some((url, depth, label)) = pending.pop_front() {
        if depth > 6 || steps >= MAX_STEPS || !visited.insert(url.to_string()) {
            continue;
        }
        steps += 1;
        if is_video_seed(&url) && !is_media_url(&url) {
            if let Some(link) = instant_link(client, &url).await.into_iter().next() {
                pending.push_front((link, depth + 1, "Instant".into()));
            }
            continue;
        }
        if let Some(link) = unwrap_url(&url) {
            pending.push_front((link, depth + 1, label));
            continue;
        }
        if url.query_pairs().any(|(key, _)| key == "sid") {
            match bypass_cloud(client, &url).await {
                Ok(link) => pending.push_front((link, depth + 1, label)),
                Err(error) if site::is_browser_verification(&error) => {
                    verification_error = Some(error);
                }
                Err(_) => {}
            }
            continue;
        }
        if is_media_url(&url) {
            if let Ok(url) = preflight(client, &url, mirror, intent).await {
                return Ok((url, label));
            }
            continue;
        }
        let page = match fetch_page(client, &url, None, None).await {
            Ok(page) => page,
            Err(error) => {
                if site::is_browser_verification(&error) {
                    verification_error = Some(error);
                }
                continue;
            }
        };
        let Some(html) = page.html else {
            if let Ok(url) = preflight(client, &page.url, mirror, intent).await {
                return Ok((url, label));
            }
            continue;
        };
        if is_video_seed(&page.url) {
            pending.push_front((page.url, depth + 1, label));
            continue;
        }
        if let Some(link) = unwrap_url(&page.url) {
            pending.push_front((link, depth + 1, label));
            continue;
        }
        if let Some(next) =
            script_redirect(&page.url, &html).or_else(|| refresh_url(&page.url, &html))
        {
            pending.push_front((next, depth + 1, label));
            continue;
        }
        let mut buttons = drive_buttons(&page.url, &html);
        // Driveleech/Driveseed: prefer resumable worker links for downloads, instant for
        // playback. Other buttons are tried if the preferred server is unavailable.
        buttons.sort_by_key(|(name, _)| {
            let instant = name.contains("instant download");
            let resume = name.contains("resume cloud");
            match intent {
                ResolutionIntent::Playback => {
                    if instant {
                        0
                    } else if resume {
                        1
                    } else {
                        2
                    }
                }
                ResolutionIntent::Download => {
                    if resume {
                        0
                    } else if instant {
                        1
                    } else {
                        2
                    }
                }
            }
        });
        let mut deferred = Vec::new();
        let mut found_buttons = false;
        for (text, target) in buttons {
            let candidates: Vec<(Url, String)> = if text.contains("instant download") {
                instant_link(client, &target)
                    .await
                    .into_iter()
                    .map(|url| (url, "Instant".into()))
                    .collect()
            } else if text.contains("resume cloud") {
                resume_cloud(client, &target)
                    .await
                    .into_iter()
                    .map(|url| (url, "Resume Cloud".into()))
                    .collect()
            } else if text.contains("direct links") {
                direct_links(client, &target)
                    .await
                    .into_iter()
                    .map(|url| (url, "Direct".into()))
                    .collect()
            } else if text.contains("resume worker bot") {
                resume_bot(client, &target)
                    .await
                    .into_iter()
                    .map(|url| (url, "Resume Bot".into()))
                    .collect()
            } else if text.contains("cloud download") {
                vec![(target, "Cloud".into())]
            } else {
                vec![]
            };
            for (next, name) in candidates {
                found_buttons = true;
                // Stop as soon as a button yields media; don't wait for every dead mirror.
                if let Ok(media) = preflight(client, &next, mirror, intent).await {
                    return Ok((media, name));
                }
                if !is_media_url(&next) {
                    deferred.push((next, depth + 1, name));
                }
            }
        }
        if !found_buttons {
            deferred.extend(
                links_with_selector(
                    &page.url,
                    &html,
                    "a.maxbutton-1[href], a.maxbutton-5[href], a.btn-success[href]",
                )
                .into_iter()
                .map(|url| (url, depth + 1, label.clone())),
            );
        }
        pending.extend(deferred);
    }
    Err(verification_error
        .unwrap_or_else(|| ProviderError::Unavailable("No direct media URL found".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_future_is_send_for_spawned_playback_and_download() {
        fn assert_send<T: Send>(_: T) {}

        let client = site::browser_client().unwrap();
        let release = site::release(
            ProviderKind::Moviesmod,
            "Film.1080p",
            "https://driveseed.example/file/abc",
            None,
        )
        .unwrap();
        assert_send(resolve_release(
            &client,
            &release,
            ProviderKind::Moviesmod,
            ResolutionIntent::Playback,
        ));
        assert_send(resolve_release(
            &client,
            &release,
            ProviderKind::Moviesmod,
            ResolutionIntent::Download,
        ));
    }

    #[tokio::test]
    async fn generic_media_responses_cannot_disguise_html_as_video() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 1024];
                let count = stream.read(&mut request).await.unwrap();
                let line = String::from_utf8_lossy(&request[..count]);
                let path = line.split_whitespace().nth(1).unwrap();
                let (mime, body): (&str, &[u8]) = match path {
                    "/bad.mkv" => ("", b"<html>Not a video</html>"),
                    "/generic.mkv" => (
                        "Content-Type: application/octet-stream\r\n",
                        b"<html>I'm Not a Robot. Click here to continue</html>",
                    ),
                    "/good.mkv" => (
                        "Content-Type: application/octet-stream\r\n",
                        b"\x1a\x45\xdf\xa3",
                    ),
                    "/manifest.mpd" => (
                        "Content-Type: application/octet-stream\r\n",
                        b"<?xml version='1.0'?><MPD></MPD>",
                    ),
                    _ => panic!("Unexpected mock request: {path}"),
                };
                let headers = format!(
                    "HTTP/1.1 206 Partial Content\r\n{mime}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(body).await.unwrap();
            }
        });
        let client = site::browser_client().unwrap();
        let mirror = SourceMirror {
            label: "Mirror".into(),
            resolver_url: format!("{base}/bad.mkv"),
            headers: vec![],
            direct_file: false,
        };
        let bad = Url::parse(&format!("{base}/bad.mkv")).unwrap();
        assert!(
            preflight(&client, &bad, &mirror, ResolutionIntent::Playback)
                .await
                .is_err()
        );
        let generic = Url::parse(&format!("{base}/generic.mkv")).unwrap();
        let gate = preflight(&client, &generic, &mirror, ResolutionIntent::Download)
            .await
            .unwrap_err();
        assert!(site::is_browser_verification(&gate));
        for path in ["good.mkv", "manifest.mpd"] {
            let url = Url::parse(&format!("{base}/{path}")).unwrap();
            assert_eq!(
                preflight(&client, &url, &mirror, ResolutionIntent::Playback)
                    .await
                    .unwrap(),
                url
            );
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn verification_gate_is_reported_but_other_mirrors_are_still_tried() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0; 1024];
                let count = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..count]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let (status, mime, body) = match path {
                    "/?sid=blocked" => (
                        "200 OK",
                        "text/html",
                        "I'm Not a Robot. Click here to continue",
                    ),
                    "/media.mkv" => ("206 Partial Content", "video/x-matroska", "x"),
                    _ => panic!("Unexpected mock request: {path}"),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client = site::browser_client().unwrap();
        let mut release = site::release(
            ProviderKind::UhdMovies,
            "Film.1080p.UHD",
            &format!("{base}/?sid=blocked"),
            None,
        )
        .unwrap();
        let gate = resolve_release(
            &client,
            &release,
            ProviderKind::UhdMovies,
            ResolutionIntent::Playback,
        )
        .await
        .unwrap_err();
        assert!(site::is_browser_verification(&gate));
        release.mirrors.push(SourceMirror {
            label: "Direct".into(),
            resolver_url: format!("{base}/media.mkv"),
            headers: Vec::new(),
            direct_file: false,
        });
        let source = resolve_release(
            &client,
            &release,
            ProviderKind::UhdMovies,
            ResolutionIntent::Download,
        )
        .await
        .unwrap();
        assert_eq!(source.url, format!("{base}/media.mkv"));
        server.await.unwrap();
    }

    #[test]
    fn decodes_safe_base64_and_percent_wrappers() {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode("https://driveseed.example/file/abc");
        let wrapped = Url::parse(&format!("https://moviesmod.example/?url={encoded}")).unwrap();
        assert_eq!(
            unwrap_url(&wrapped).unwrap().as_str(),
            "https://driveseed.example/file/abc"
        );
        let encoded =
            Url::parse("https://video-seed.xyz/?url=https%3A%2F%2Fcdn.example%2Fv.mkv").unwrap();
        assert_eq!(
            unwrap_url(&encoded).unwrap().as_str(),
            "https://cdn.example/v.mkv"
        );
        let malicious =
            Url::parse("https://moviesmod.example/?url=ZmlsZTovLy9ldGMvcGFzc3dk").unwrap();
        assert!(unwrap_url(&malicious).is_none());
    }

    #[test]
    fn parses_two_landing_forms_and_drive_redirect() {
        let base = Url::parse("https://cloud.unblockedgames.world/?sid=abc").unwrap();
        let (url, fields) = landing_form(
            &base,
            r#"<form id="landing" action="/first"><input name="_wp_http" value="sid"></form>"#,
        )
        .unwrap();
        assert_eq!(url.path(), "/first");
        assert_eq!(fields, vec![("_wp_http".into(), "sid".into())]);
        assert!(landing_form(&base, r#"<form id="landing" action="https://evil.example/collect"><input name="token" value="private"></form>"#).is_none());
        assert_eq!(
            go_token("<script>s_10('?go=pepe-hash','value',60)</script>").as_deref(),
            Some("pepe-hash")
        );
        assert_eq!(
            refresh_url(
                &base,
                r#"<meta http-equiv="refresh" content="0;url=https://driveseed.org/r?key=1">"#
            )
            .unwrap()
            .host_str(),
            Some("driveseed.org")
        );
        let drive = Url::parse("https://driveseed.org/r?key=1").unwrap();
        assert_eq!(
            script_redirect(
                &drive,
                r#"<script>window.location.replace("/file/key")</script>"#
            )
            .unwrap()
            .path(),
            "/file/key"
        );
    }

    #[test]
    fn recognizes_current_gate_forms_and_cookie_hop() {
        let base = Url::parse("https://en.gate.example/?sid=abc").unwrap();
        let (url, fields) = landing_form(
            &base,
            r#"<form id="lp-land" method="POST" action="https://en.gate.example/"><input type="hidden" name="_lp_http" value="v1"></form>"#,
        )
        .unwrap();
        assert_eq!(url.path(), "/");
        assert_eq!(fields, vec![("_lp_http".into(), "v1".into())]);
        // Search forms and cross-origin forms are never treated as gates.
        assert!(
            landing_form(
                &base,
                r#"<form role="search" action="/"><input name="s" value=""></form>"#
            )
            .is_none()
        );
        assert!(landing_form(&base, r#"<form id="lp-x" action="https://evil.example/"><input name="_lp_http" value="v"></form>"#).is_none());
        let (go, name, value) = lp_go(
            &base,
            r#"<script>sc("lp-1f","eJy\/Zz==",60); bg.setAttribute('href',"https:\/\/en.gate.example\/?lp_go=lp-1f");</script>"#,
        )
        .unwrap();
        assert_eq!(go.as_str(), "https://en.gate.example/?lp_go=lp-1f");
        assert_eq!((name.as_str(), value.as_str()), ("lp-1f", "eJy/Zz=="));
        assert!(lp_go(&base, "<p>no gate here</p>").is_none());
    }

    #[tokio::test]
    async fn walks_current_linkpilot_gate_to_the_file_host() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let target_base = format!("http://{}", other.local_addr().unwrap());
        let hops = Arc::new(AtomicUsize::new(0));
        let seen = hops.clone();
        let (server_base, destination) = (base.clone(), target_base.clone());
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut data = [0; 8192];
                let Ok(n) = stream.read(&mut data).await else {
                    continue;
                };
                let request = String::from_utf8_lossy(&data[..n]).to_string();
                let mut first = request.split_whitespace();
                let (method, path) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
                let body = match (method, path) {
                    ("GET", "/?sid=abc") => format!(
                        "<form id='lp-land' method='POST' action='{server_base}/'><input type='hidden' name='_lp_http' value='v1'></form>"
                    ),
                    ("POST", "/") if request.contains("_lp_http=v1") => {
                        seen.fetch_add(1, Ordering::SeqCst);
                        format!(
                            "<form role='search' action='{server_base}/'><input name='s' value=''></form>\
                             <form id='lp-s1-form' method='POST' action='{server_base}/fake-post/'>\
                             <input type='hidden' name='_lp_http2' value='v2'><input type='hidden' name='_lp_hop_index' value='1'></form>"
                        )
                    }
                    ("POST", "/fake-post/")
                        if request.contains("_lp_http2=v2")
                            && request.contains("_lp_hop_index=1") =>
                    {
                        seen.fetch_add(1, Ordering::SeqCst);
                        r#"<script>sc("lp-9z","cookie\/value",60); x.setAttribute('href',"https:\/\/x\/?lp_go=lp-9z");</script>"#.to_string()
                    }
                    ("GET", "/?lp_go=lp-9z") if request.contains("lp-9z=cookie/value") => {
                        seen.fetch_add(1, Ordering::SeqCst);
                        format!(
                            "<script>setTimeout(function(){{window.location.replace(\"{destination}\\/r?key=k\");}},350);</script>"
                        )
                    }
                    _ => String::new(),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        let client = site::browser_client().unwrap();
        let sid = Url::parse(&format!("{base}/?sid=abc")).unwrap();
        let target = bypass_cloud(&client, &sid).await.unwrap();
        assert_eq!(target.as_str(), format!("{target_base}/r?key=k"));
        assert_eq!(hops.load(Ordering::SeqCst), 3);
        // An unrecognized gate is a clear error, not a hang or a wrong URL.
        let unknown = Url::parse(&format!("{base}/?sid=none")).unwrap();
        assert!(bypass_cloud(&client, &unknown).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn resolves_cloud_and_moviesmod_pages_to_video_for_both_intents() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let cookie_ok = Arc::new(AtomicBool::new(false));
        let checked = cookie_ok.clone();
        let server_base = base.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut data = [0; 8192];
                let Ok(n) = stream.read(&mut data).await else {
                    continue;
                };
                let request = String::from_utf8_lossy(&data[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let body = match path {
                    "/?sid=abc" => "<form id='landing' action='/first'><input name='_wp_http' value='abc'></form>".to_string(),
                    "/first" => "<form id='landing' action='/second'><input name='_wp_http2' value='secret'></form>".to_string(),
                    "/second" => "<script>s_1('?go=token','secret',60)</script>".to_string(),
                    "/?go=token" => {
                        checked.store(request.contains("token=secret"), Ordering::SeqCst);
                        format!("<meta http-equiv='refresh' content='0;url={server_base}/r?key=abc'>")
                    },
                    "/r?key=abc" => "<script>window.location.replace(\"/file/abc\")</script>".to_string(),
                    "/file/abc" => "<div class='text-center'><a href='/instant'>Instant Download</a><a href='/zfile/abc'>Resume Cloud</a></div>".to_string(),
                    "/file/bot" => "<div class='text-center'><a href='/bot'>Resume Worker Bot</a></div>".to_string(),
                    "/bot" => "<script>formData.append('token', 'abc123'); fetch('/download?id=YWJj')</script>".to_string(),
                    "/download?id=YWJj" if request.contains("PHPSESSID=session") => format!("{{\"url\":\"{server_base}/media.mkv\"}}"),
                    "/zfile/abc" => "<a class='btn-success' href='/media.mkv'>Worker</a>".to_string(),
                    "/media.mkv" => "x".to_string(),
                    "/bad.mkv" => "<html>not video</html>".to_string(),
                    _ if path.starts_with("/video-seed/") => "<html>redirect</html>".to_string(),
                    _ => String::new(),
                };
                let (status, extra) = if path == "/instant" {
                    let mut target = Url::parse(&format!("{server_base}/video-seed/")).unwrap();
                    target
                        .query_pairs_mut()
                        .append_pair("url", &format!("{server_base}/media.mkv"));
                    ("HTTP/1.1 302 Found", format!("Location: {target}\r\n"))
                } else if path == "/media.mkv" {
                    (
                        "HTTP/1.1 206 Partial Content",
                        "Content-Type: video/x-matroska\r\n".to_string(),
                    )
                } else if path == "/bad.mkv" {
                    ("HTTP/1.1 200 OK", "Content-Type: text/html\r\n".to_string())
                } else if path == "/bot" {
                    (
                        "HTTP/1.1 200 OK",
                        "Content-Type: text/html\r\nSet-Cookie: PHPSESSID=session; Path=/\r\n"
                            .to_string(),
                    )
                } else if path.starts_with("/download?id=") {
                    (
                        "HTTP/1.1 200 OK",
                        "Content-Type: application/json\r\n".to_string(),
                    )
                } else {
                    ("HTTP/1.1 200 OK", "Content-Type: text/html\r\n".to_string())
                };
                let response = format!(
                    "{status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let client = site::browser_client().unwrap();
        let release = |provider, url: String| Release {
            provider,
            filename: "Demo.1080p.mkv".into(),
            quality: Some("1080p".into()),
            codec: None,
            language: None,
            size_bytes: None,
            season: None,
            episode: None,
            mirrors: vec![SourceMirror {
                label: "Drive".into(),
                resolver_url: url,
                headers: vec![],
                direct_file: false,
            }],
            resource_id: None,
        };
        let uhd = release(ProviderKind::UhdMovies, format!("{base}/?sid=abc"));
        let download = resolve_release(
            &client,
            &uhd,
            ProviderKind::UhdMovies,
            ResolutionIntent::Download,
        )
        .await
        .unwrap();
        assert_eq!(download.url, format!("{base}/media.mkv"));
        assert_eq!(download.provider, ProviderKind::UhdMovies);
        assert!(
            download
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
        );
        assert!(cookie_ok.load(Ordering::SeqCst));
        let playback = resolve_release(
            &client,
            &uhd,
            ProviderKind::UhdMovies,
            ResolutionIntent::Playback,
        )
        .await
        .unwrap();
        assert_eq!(playback.url, format!("{base}/media.mkv"));

        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{base}/file/abc"));
        let moviesmod = release(ProviderKind::Moviesmod, format!("{base}/?url={encoded}"));
        let source = resolve_release(
            &client,
            &moviesmod,
            ProviderKind::Moviesmod,
            ResolutionIntent::Download,
        )
        .await
        .unwrap();
        assert_eq!(source.url, format!("{base}/media.mkv"));
        let bot = release(ProviderKind::Moviesmod, format!("{base}/file/bot"));
        let source = resolve_release(
            &client,
            &bot,
            ProviderKind::Moviesmod,
            ResolutionIntent::Playback,
        )
        .await
        .unwrap();
        assert_eq!(source.url, format!("{base}/media.mkv"));
        let mut invalid = release(ProviderKind::Moviesmod, format!("{base}/bad.mkv"));
        assert!(
            resolve_release(
                &client,
                &invalid,
                ProviderKind::Moviesmod,
                ResolutionIntent::Download
            )
            .await
            .is_err()
        );
        invalid.mirrors.push(SourceMirror {
            label: "Backup".into(),
            resolver_url: format!("{base}/media.mkv"),
            headers: vec![],
            direct_file: false,
        });
        let backup = resolve_release(
            &client,
            &invalid,
            ProviderKind::Moviesmod,
            ResolutionIntent::Download,
        )
        .await
        .unwrap();
        assert_eq!(backup.url, format!("{base}/media.mkv"));
        server.abort();
    }

    #[tokio::test]
    async fn rejects_foreign_releases_without_network_calls() {
        let release = Release {
            provider: ProviderKind::MovieBox,
            filename: "Movie".into(),
            quality: None,
            codec: None,
            language: None,
            size_bytes: None,
            season: None,
            episode: None,
            mirrors: vec![],
            resource_id: None,
        };
        let client = site::browser_client().unwrap();
        assert!(
            resolve_release(
                &client,
                &release,
                ProviderKind::Moviesmod,
                ResolutionIntent::Download
            )
            .await
            .is_err()
        );
    }
}
