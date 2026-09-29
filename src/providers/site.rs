//! Small, shared helpers for WordPress-backed providers. URLs from posts are untrusted.
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{StatusCode, Url};
use scraper::{ElementRef, Html, Selector};
use tokio::sync::Mutex;
use tokio::time::Instant;
use url::Host;

use super::models::{Episode, ProviderError, ProviderKind, Release, Season, SourceMirror};

pub(super) fn base_url(raw: &str) -> Result<Url, ProviderError> {
    let mut url = safe_url(raw)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ProviderError::Parsing("Invalid provider base URL".into()));
    }
    url.set_path("/");
    Ok(url)
}

/// A completed request can have a query string even though a configured base must not.
pub(super) fn response_base(url: &Url) -> Result<Url, ProviderError> {
    let mut base = safe_url(url.as_str())?;
    base.set_path("/");
    base.set_query(None);
    base.set_fragment(None);
    Ok(base)
}

pub(super) fn safe_url(raw: &str) -> Result<Url, ProviderError> {
    if raw.len() > 8192 {
        return Err(ProviderError::Parsing("Source URL is too long".into()));
    }
    let url = Url::parse(raw).map_err(|_| ProviderError::Parsing("Invalid source URL".into()))?;
    let host = url
        .host()
        .ok_or_else(|| ProviderError::Parsing("Source URL has no host".into()))?;
    let (is_local, is_loopback_ip) = match host {
        Host::Ipv4(ip) => (!public_ip(IpAddr::V4(ip)), ip.is_loopback()),
        Host::Ipv6(ip) => (!public_ip(IpAddr::V6(ip)), ip.is_loopback()),
        Host::Domain(host) => {
            let host = host.trim_end_matches('.');
            (
                host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local"),
                false,
            )
        }
    };
    // Only loopback IPs used by the mocked HTTP servers are permitted in tests.
    // Do not exempt arbitrary local hosts or private subnets from URL validation.
    let test_loopback = cfg!(test) && is_loopback_ip;
    if (url.scheme() != "https" && !(test_loopback && url.scheme() == "http"))
        || (is_local && !test_loopback)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ProviderError::Parsing("Unsafe source URL".into()));
    }
    Ok(url)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation())
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(mapped));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

pub(super) fn same_site_url(base: &Url, href: &str) -> Result<Url, ProviderError> {
    // Reject scheme-relative and absolute URLs as ids (including encoded path traversal).
    if href.starts_with("//") || href.contains('\0') {
        return Err(ProviderError::Parsing("Invalid provider item URL".into()));
    }
    let url = base
        .join(href)
        .map_err(|_| ProviderError::Parsing("Invalid provider item URL".into()))?;
    if url.origin() != base.origin() || url.path().split('/').any(|part| part == "..") {
        return Err(ProviderError::Parsing(
            "Item belongs to another provider".into(),
        ));
    }
    Ok(url)
}

pub(super) fn item_url(base: &Url, id: &str) -> Result<Url, ProviderError> {
    if !id.starts_with('/')
        || id.starts_with("//")
        || id.contains(['?', '#', '\\'])
        || id.contains("..")
        || id.to_ascii_lowercase().contains("%2e")
    {
        return Err(ProviderError::Parsing("Invalid provider item id".into()));
    }
    let url = same_site_url(base, id)?;
    if url.path() == "/" || url.path().contains("..") {
        return Err(ProviderError::Parsing("Invalid provider item id".into()));
    }
    Ok(url)
}

/// Accept a permalink from a previously trusted provider domain, but always fetch its path
/// from the *current* domain. Never fetch an arbitrary external search-result link.
pub(super) fn migrated_item_url(base: &Url, aliases: &[Url], href: &str) -> Option<Url> {
    if href.starts_with("//")
        || href.contains(['\\', '\0'])
        || href.contains("..")
        || href.to_ascii_lowercase().contains("%2e")
    {
        return None;
    }
    let url = base.join(href).ok()?;
    if url.origin() == base.origin() {
        return same_site_url(base, href).ok();
    }
    if url.query().is_some()
        || url.fragment().is_some()
        || safe_url(url.as_str()).is_err()
        || !aliases.iter().any(|alias| alias.origin() == url.origin())
    {
        return None;
    }
    item_url(base, url.path()).ok()
}

/// Rebase a *known* old endpoint's link onto its configured successor. Preserve the path and
/// query (worker tokens may be signed), but never use an untrusted link as a new HTTP origin.
pub(super) fn rebase_url(from: &Url, to: &Url, url: &Url) -> Option<Url> {
    if url.origin() != from.origin() || safe_url(url.as_str()).is_err() {
        return None;
    }
    let mut rebased = to.clone();
    rebased.set_path(url.path());
    rebased.set_query(url.query());
    rebased.set_fragment(None);
    Some(rebased)
}

/// WordPress button targets sometimes remain absolute URLs on an older site. Move only
/// explicitly trusted provider origins; third-party file hosts are not interchangeable.
pub(super) fn provider_link(base: &Url, aliases: &[Url], href: &str) -> Option<Url> {
    let url = external_url(base, href)?;
    let rebased = aliases
        .iter()
        .find(|old| old.origin() == url.origin() && old.origin() != base.origin())
        .and_then(|old| rebase_url(old, base, &url));
    Some(rebased.unwrap_or(url))
}

pub(super) fn returned_item_base(id: &str, response_url: &Url) -> Result<Url, ProviderError> {
    let base = response_base(response_url)?;
    let requested = item_url(&base, id)?;
    if requested.path().trim_end_matches('/') != response_url.path().trim_end_matches('/') {
        return Err(ProviderError::Unavailable(
            "Provider post redirected to a different page".into(),
        ));
    }
    Ok(base)
}

pub(super) fn network_error(error: reqwest::Error) -> ProviderError {
    // These links often contain signed tokens. reqwest errors otherwise include the full URL.
    ProviderError::Network(error.without_url().to_string())
}

pub(super) fn check_page(html: &str) -> Result<(), ProviderError> {
    let lower = html.to_ascii_lowercase();
    if lower.contains("cf-chl-")
        || (lower.contains("just a moment") && lower.contains("cloudflare"))
        || (lower.contains("not a robot") && lower.contains("click here to continue"))
    {
        return Err(ProviderError::Unavailable(
            "Browser verification required by site; try another mirror".into(),
        ));
    }
    Ok(())
}

pub(super) fn is_browser_verification(error: &ProviderError) -> bool {
    matches!(error, ProviderError::Unavailable(message) if message.starts_with("Browser verification required"))
}

/// Consult a Cloudstream extension's domain list. Invalid, unavailable, or slow lists
/// cannot replace a last-known working origin.
async fn discover_domain(client: &reqwest::Client, domain_list: &str, key: &str) -> Option<Url> {
    let discovered = async {
        let data = client
            .get(domain_list)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json::<serde_json::Value>()
            .await
            .ok()?;
        base_url(data.get(key)?.as_str()?).ok()
    };
    tokio::time::timeout(Duration::from_secs(3), discovered)
        .await
        .ok()
        .flatten()
}

const DOMAIN_REFRESH: Duration = Duration::from_secs(15 * 60);
const FAILURE_REFRESH: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(super) struct DomainSource {
    fallback: Url,
    list: Option<(String, &'static str)>,
    state: Arc<Mutex<DomainState>>,
}

struct DomainState {
    current: Url,
    listed: Option<Url>,
    redirected_from: Option<Url>,
    checked: Option<Instant>,
    failure_checked: Option<Instant>,
    previous: Vec<Url>,
}

impl DomainState {
    fn change_to(&mut self, next: Url) {
        if self.current.origin() == next.origin() {
            return;
        }
        log::info!(
            "WordPress provider origin changed: {} -> {}",
            self.current,
            next
        );
        if !self
            .previous
            .iter()
            .any(|url| url.origin() == self.current.origin())
        {
            self.previous.push(self.current.clone());
            if self.previous.len() > 4 {
                self.previous.remove(0);
            }
        }
        self.current = next;
    }

    fn apply_listing(&mut self, found: Url, recovering: bool) {
        // A newly reachable but lagging list must not undo a verified HTTP migration.
        // If that migrated origin itself fails, it is safe to try the listed origin once.
        let stale_redirect = self
            .redirected_from
            .as_ref()
            .is_some_and(|old| old.origin() == found.origin());
        if self.current.origin() != found.origin()
            && (recovering || (self.listed.as_ref() != Some(&found) && !stale_redirect))
        {
            self.change_to(found.clone());
            self.redirected_from = None;
        } else if self.current.origin() == found.origin() {
            self.redirected_from = None;
        }
        self.listed = Some(found);
    }
}

impl DomainSource {
    pub fn fixed(base: Url) -> Self {
        Self {
            fallback: base.clone(),
            list: None,
            state: Arc::new(Mutex::new(DomainState {
                current: base,
                listed: None,
                redirected_from: None,
                checked: None,
                failure_checked: None,
                previous: Vec::new(),
            })),
        }
    }

    pub fn discovering(base: Url, list: &str, key: &'static str) -> Self {
        let mut source = Self::fixed(base);
        source.list = Some((list.to_string(), key));
        source
    }

    pub async fn base(&self, client: &reqwest::Client) -> Url {
        let mut state = self.state.lock().await;
        if let Some((list, key)) = &self.list
            && state
                .checked
                .is_none_or(|checked| checked.elapsed() >= DOMAIN_REFRESH)
        {
            state.checked = Some(Instant::now());
            if let Some(found) = discover_domain(client, list, key).await {
                state.apply_listing(found, false);
            }
        }
        state.current.clone()
    }

    async fn refresh_after_failure(&self, client: &reqwest::Client, failed: &Url) -> Option<Url> {
        let (list, key) = self.list.as_ref()?;
        let mut state = self.state.lock().await;
        if state.current.origin() != failed.origin() {
            return Some(state.current.clone()); // another request discovered the new origin
        }
        if state
            .failure_checked
            .is_some_and(|checked| checked.elapsed() < FAILURE_REFRESH)
        {
            return None;
        }
        state.failure_checked = Some(Instant::now());
        state.checked = Some(Instant::now());
        if let Some(found) = discover_domain(client, list, key).await {
            state.apply_listing(found, true);
        }
        (state.current.origin() != failed.origin()).then(|| state.current.clone())
    }

    /// Retry a failed request at most once if the discovery list changed. A fixed URL still
    /// learns a new origin from an HTTP redirect, but is never silently replaced by a list.
    pub async fn get(
        &self,
        client: &reqwest::Client,
        make_url: impl Fn(&Url) -> Result<Url, ProviderError>,
        retry_not_found: bool,
    ) -> Result<(Url, reqwest::Response), ProviderError> {
        let mut base = self.base(client).await;
        for attempt in 0..2 {
            let url = make_url(&base)?;
            match client.get(url).send().await {
                Ok(response) => {
                    let retry = response.status().is_server_error()
                        || response.status() == StatusCode::FORBIDDEN
                        || response.status() == StatusCode::GONE
                        || (retry_not_found && response.status() == StatusCode::NOT_FOUND);
                    if attempt == 0
                        && retry
                        && let Some(next) = self.refresh_after_failure(client, &base).await
                    {
                        base = next;
                        continue;
                    }
                    return Ok((base, response));
                }
                Err(error) => {
                    if attempt == 0
                        && let Some(next) = self.refresh_after_failure(client, &base).await
                    {
                        base = next;
                        continue;
                    }
                    return Err(network_error(error));
                }
            }
        }
        unreachable!("domain requests have at most one retry")
    }

    /// Only call after the final response has been checked and recognized as the requested
    /// provider page. This keeps ad/verification redirects from poisoning the provider origin.
    pub async fn accept_redirect(&self, requested: &Url, effective: &Url) {
        let mut state = self.state.lock().await;
        if state.current.origin() == requested.origin()
            && state.current.origin() != effective.origin()
        {
            state.change_to(effective.clone());
            state.redirected_from = Some(requested.clone());
        }
    }

    pub async fn aliases(&self, requested: &Url) -> Vec<Url> {
        let state = self.state.lock().await;
        let mut aliases = state.previous.clone();
        aliases.push(self.fallback.clone());
        aliases.push(requested.clone());
        if let Some(listed) = &state.listed {
            aliases.push(listed.clone());
        }
        aliases
    }
}

pub(super) fn browser_client() -> Result<reqwest::Client, ProviderError> {
    crate::net::http_client_builder()
        .user_agent(crate::net::DEFAULT_BROWSER_USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(18))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || safe_url(attempt.url().as_str()).is_err() {
                attempt.error("Unsafe or excessive redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(network_error)
}

pub(super) fn external_url(base: &Url, href: &str) -> Option<Url> {
    let url = base.join(href).ok()?;
    safe_url(url.as_str()).ok()
}

pub(super) fn previous_element(node: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut sibling = node.prev_sibling();
    while let Some(current) = sibling {
        sibling = current.prev_sibling();
        if let Some(element) = ElementRef::wrap(current) {
            return Some(element);
        }
    }
    None
}

pub(super) fn preceding_quality(node: ElementRef<'_>) -> Option<String> {
    let previous = previous_element(node)?;
    if !matches!(previous.value().name(), "strong" | "b") {
        return None;
    }
    let label = text(Some(previous))?;
    quality(&label).map(|_| label)
}

pub(super) fn text(node: Option<ElementRef<'_>>) -> Option<String> {
    node.map(|node| node.text().collect::<Vec<_>>().join(" "))
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty())
}

pub(super) fn attr(document: &Html, selector: &str, attribute: &str) -> Option<String> {
    document
        .select(&Selector::parse(selector).ok()?)
        .next()?
        .value()
        .attr(attribute)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn clean_title(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("Download ")
        .or_else(|| raw.strip_prefix("download "))
        .unwrap_or(raw);
    raw.split(" || ").next().unwrap_or(raw).trim().to_string()
}

pub(super) fn year(raw: &str) -> Option<String> {
    // Resolution tags such as 2160p are not release years.
    raw.as_bytes()
        .windows(4)
        .filter_map(|digits| std::str::from_utf8(digits).ok()?.parse::<u16>().ok())
        .find(|&value| (1900..=2100).contains(&value))
        .map(|value| value.to_string())
}

fn number_after(raw: &str, prefix: &str) -> Option<usize> {
    let lower = raw.to_ascii_lowercase();
    let mut start = 0;
    while let Some(offset) = lower[start..].find(prefix) {
        let idx = start + offset;
        start = idx + prefix.len();
        if idx > 0 && lower.as_bytes()[idx - 1].is_ascii_alphanumeric() {
            continue;
        }
        let digits = lower[start..].trim_start_matches([' ', ':', '-', '#']);
        let len = digits.bytes().take_while(u8::is_ascii_digit).count();
        if len > 0 {
            return digits[..len].parse().ok();
        }
    }
    None
}

pub(super) fn season_number(text: &str) -> Option<usize> {
    number_after(text, "season")
        .or_else(|| number_after(text, "s").filter(|&number| number <= 99))
        .filter(|&number| number > 0)
}

pub(super) fn episode_marker(text: &str, default_season: usize) -> Option<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b's' || (i > 0 && bytes[i - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let mut next = i + 1;
        while next < bytes.len() && bytes[next].is_ascii_digit() {
            next += 1;
        }
        if next == i + 1 || next >= bytes.len() || bytes[next] != b'e' {
            continue;
        }
        let mut end = next + 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end > next + 1
            && let (Ok(season), Ok(episode)) = (
                lower[i + 1..next].parse::<usize>(),
                lower[next + 1..end].parse::<usize>(),
            )
            && season > 0
            && episode > 0
        {
            return Some((season, episode));
        }
    }
    let episode = number_after(text, "episode").or_else(|| number_after(text, "ep"))?;
    (episode > 0).then_some((
        season_number(text).unwrap_or(default_season.max(1)),
        episode,
    ))
}

pub(super) fn quality(label: &str) -> Option<String> {
    let lower = label.to_ascii_lowercase();
    if lower.contains("2160p") || lower.contains("4k") {
        return Some("2160p".into());
    }
    // UHDMovies calls some 1080p encodes "1080p UHD". An explicit resolution is
    // more trustworthy than the site's generic UHD branding.
    ["1440p", "1080p", "720p", "480p", "360p"]
        .into_iter()
        .find(|q| lower.contains(q))
        .map(str::to_string)
        .or_else(|| {
            lower
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| word == "uhd")
                .then(|| "2160p".into())
        })
}

pub(super) fn release(
    provider: ProviderKind,
    filename: &str,
    href: &str,
    season_episode: Option<(usize, usize)>,
) -> Option<Release> {
    let url = safe_url(href).ok()?;
    let filename = filename.split_whitespace().collect::<Vec<_>>().join(" ");
    if filename.is_empty()
        || filename.to_ascii_lowercase().contains(".zip")
        || url.path().to_ascii_lowercase().ends_with(".zip")
    {
        return None;
    }
    Some(Release {
        provider,
        quality: quality(&filename),
        codec: super::bdix::common::detect_codec(&filename),
        language: super::bdix::common::detect_audio_language(&filename),
        size_bytes: super::models::parse_size_bytes(&filename),
        season: season_episode.map(|(season, _)| season),
        episode: season_episode.map(|(_, episode)| episode),
        filename,
        mirrors: vec![SourceMirror {
            label: provider.label().to_string(),
            resolver_url: url.to_string(),
            headers: Vec::new(),
            direct_file: false, // all mirrors are checked and resolved before playback/download
        }],
        resource_id: None,
    })
}

pub(super) fn seasons(releases: &[Release]) -> Vec<Season> {
    let mut seasons = BTreeMap::<usize, BTreeMap<usize, Episode>>::new();
    for release in releases {
        if let (Some(season), Some(number)) = (release.season, release.episode) {
            seasons
                .entry(season)
                .or_default()
                .entry(number)
                .or_insert(Episode {
                    season,
                    number,
                    title: None,
                    overview: None,
                });
        }
    }
    seasons
        .into_iter()
        .map(|(number, episodes)| Season {
            number,
            episodes: episodes.into_values().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_ids_cannot_escape_the_provider() {
        let base = base_url("https://movies.example/").unwrap();
        assert!(item_url(&base, "/download-foo/").is_ok());
        for id in [
            "//evil.example/post",
            "https://evil.example/post",
            "/../admin",
            "/post?x=1",
        ] {
            assert!(item_url(&base, id).is_err(), "{id}");
        }
        assert!(safe_url("http://example.com/file.mkv").is_err());
        assert!(safe_url("https://127.0.0.1/admin").is_ok()); // test-only mock URLs
        assert!(safe_url("http://127.0.0.1:1234/mock").is_ok());
        assert!(safe_url("file:///etc/passwd").is_err());
        assert!(safe_url("https://admin.local/secrets").is_err());
        assert!(safe_url("http://admin.local/secrets").is_err());
        assert!(safe_url("https://192.168.0.1/secrets").is_err());
        assert!(safe_url("https://localhost/secrets").is_err());
        assert!(!public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(check_page("<title>Just a moment...</title>Cloudflare").is_err());
        assert!(check_page("I'm Not a Robot. Click here to continue").is_err());
    }

    #[test]
    fn known_old_links_are_rebased_but_unknown_origins_are_rejected() {
        let current = base_url("https://new.example/").unwrap();
        assert_eq!(
            response_base(&Url::parse("https://new.example/search/film/?s=film").unwrap()).unwrap(),
            current
        );
        let old = base_url("https://old.example/").unwrap();
        let aliases = [old.clone()];
        let migrated = migrated_item_url(&current, &aliases, "https://old.example/show/").unwrap();
        assert_eq!(migrated.as_str(), "https://new.example/show/");
        assert_eq!(
            migrated_item_url(&current, &aliases, "/show/").unwrap(),
            migrated
        );
        for url in [
            "https://evil.example/show/",
            "https://old.example.evil.test/show/",
            "//old.example/show/",
            "https://old.example/../admin",
            "https://old.example/show/?token=123",
        ] {
            assert!(
                migrated_item_url(&current, &aliases, url).is_none(),
                "{url}"
            );
        }
        let token = Url::parse("https://old.example/redirect?data=abc%2Bdef").unwrap();
        assert_eq!(
            rebase_url(&old, &current, &token).unwrap().as_str(),
            "https://new.example/redirect?data=abc%2Bdef"
        );
        assert!(
            rebase_url(
                &old,
                &current,
                &Url::parse("https://evil.example/x").unwrap()
            )
            .is_none()
        );
        assert_eq!(
            provider_link(&current, &aliases, "https://old.example/?sid=abc%2Bdef")
                .unwrap()
                .as_str(),
            "https://new.example/?sid=abc%2Bdef"
        );
        assert_eq!(
            provider_link(&current, &aliases, "https://drive.example/file")
                .unwrap()
                .as_str(),
            "https://drive.example/file"
        );
        assert!(returned_item_base("/show/", &migrated).is_ok());
        assert!(
            returned_item_base("/show/", &Url::parse("https://new.example/home/").unwrap())
                .is_err()
        );
    }

    #[tokio::test]
    async fn domain_discovery_prefers_the_list_and_falls_back_if_unavailable() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let list = format!("http://{}/domains.json", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 512];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            let json = r#"{"UHDMovies":"https://uhdmovies.example/","moviesmod":"https://moviesmod.example/"}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                json.len()
            );
            stream.write_all(reply.as_bytes()).await.unwrap();
        });
        let client = browser_client().unwrap();
        let fallback = base_url("https://fallback.example/").unwrap();
        let discovered = DomainSource::discovering(fallback.clone(), &list, "UHDMovies");
        assert_eq!(
            discovered.base(&client).await.host_str(),
            Some("uhdmovies.example")
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        let unavailable =
            DomainSource::discovering(fallback.clone(), "https://127.0.0.1:1/bad", "UHDMovies");
        assert_eq!(unavailable.base(&client).await, fallback);
    }

    #[tokio::test]
    async fn lagging_domain_list_cannot_undo_confirmed_redirect() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let list = format!("http://{}/domains.json", listener.local_addr().unwrap());
        let old = base_url("https://old.example/").unwrap();
        let new = base_url("https://new.example/").unwrap();
        let body = serde_json::json!({"moviesmod": old.as_str()}).to_string();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0; 1024];
                assert!(stream.read(&mut buf).await.unwrap() > 0);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client = browser_client().unwrap();
        let domain = DomainSource::discovering(old.clone(), &list, "moviesmod");
        assert_eq!(domain.base(&client).await, old);
        domain.accept_redirect(&old, &new).await;
        domain.state.lock().await.checked = None; // simulate periodic refresh
        assert_eq!(domain.base(&client).await, new);
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn failed_old_domain_rechecks_list_and_retries_new_domain_once() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let list_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let old_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let new_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let old = format!("http://{}/", old_listener.local_addr().unwrap());
        let new = format!("http://{}/", new_listener.local_addr().unwrap());
        let list = format!("http://{}/list.json", list_listener.local_addr().unwrap());
        let old_for_list = old.clone();
        let new_for_list = new.clone();
        let list_task = tokio::spawn(async move {
            for base in [old_for_list, new_for_list] {
                let (mut stream, _) = list_listener.accept().await.unwrap();
                let mut buf = [0; 1024];
                assert!(stream.read(&mut buf).await.unwrap() > 0);
                let body = serde_json::json!({"moviesmod": base}).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let old_task = tokio::spawn(async move {
            let (mut stream, _) = old_listener.accept().await.unwrap();
            let mut buf = [0; 1024];
            assert!(stream.read(&mut buf).await.unwrap() > 0);
            stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let new_task = tokio::spawn(async move {
            let (mut stream, _) = new_listener.accept().await.unwrap();
            let mut buf = [0; 1024];
            assert!(stream.read(&mut buf).await.unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        });
        let client = browser_client().unwrap();
        let domain = DomainSource::discovering(base_url(&old).unwrap(), &list, "moviesmod");
        let (base, response) = domain
            .get(&client, |base| Ok(base.join("search").unwrap()), true)
            .await
            .unwrap();
        assert_eq!(base.as_str(), new);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(domain.base(&client).await, base);
        for task in [list_task, old_task, new_task] {
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[test]
    fn parses_episode_and_release_metadata() {
        assert_eq!(episode_marker("[1080p] S02E09", 1), Some((2, 9)));
        assert_eq!(episode_marker("Season 3 Episode 12", 1), Some((3, 12)));
        assert_eq!(season_number("Show S03 1080p"), Some(3));
        assert_eq!(season_number("Movie 2160p"), None);
        assert_eq!(episode_marker("Episode 4", 2), Some((2, 4)));
        assert_eq!(episode_marker("Season 2", 1), None);
        assert_eq!(year("Film.2160p.HDR"), None);
        assert_eq!(year("Film (2026) 2160p"), Some("2026".into()));
        assert_eq!(quality("1080p UHD x265").as_deref(), Some("1080p"));
        assert_eq!(quality("2160p UHD x265").as_deref(), Some("2160p"));
        assert_eq!(quality("UHDMovies"), None);
        let r = release(
            ProviderKind::UhdMovies,
            "S02E09 4K HEVC 1.5 GB",
            "https://cdn.example/file",
            Some((2, 9)),
        )
        .unwrap();
        assert_eq!(r.quality.as_deref(), Some("2160p"));
        assert_eq!(r.season, Some(2));
        assert_eq!(r.episode, Some(9));
        assert_eq!(seasons(&[r])[0].episodes[0].number, 9);
    }
}
