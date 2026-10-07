//! ToonWorld4All's WordPress posts and the separate episode archive have different layouts.
//! Keep the link labels (and quality variants) attached to their own mirrors; none of the
//! links collected here are assumed to be playable until the resolver verifies the media.
use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;

use super::super::models::{
    CatalogItem, MediaDetails, MediaType, ProviderError, ProviderKind, ProviderMediaId, Release,
};
use super::super::{drive, site};

#[derive(Clone)]
pub(super) struct Origins {
    pub site: Url,
    pub archive: Url,
    pub worker: Url,
    pub site_aliases: Vec<Url>,
    pub archive_aliases: Vec<Url>,
    pub worker_aliases: Vec<Url>,
}

fn title(raw: &str) -> String {
    let fragment = Html::parse_fragment(raw);
    site::clean_title(&site::text(Some(fragment.root_element())).unwrap_or_default())
}

fn image(base: &Url, raw: &str) -> Option<String> {
    site::external_url(base, raw).map(|url| url.to_string())
}

fn post_url(base: &Url, aliases: &[Url], raw: &str) -> Option<Url> {
    let url = site::migrated_item_url(base, aliases, raw)?;
    (url.query().is_none()
        && url.path().trim_matches('/').split('/').count() == 1
        && !url.path().trim_matches('/').is_empty())
    .then_some(url)
}

fn is_series(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    [
        "season ",
        "episodes",
        "tv series",
        "web series",
        "complete series",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn catalog_item(
    base: &Url,
    aliases: &[Url],
    link: &str,
    raw_title: &str,
    poster: Option<String>,
) -> Option<CatalogItem> {
    let url = post_url(base, aliases, link)?;
    let title = title(raw_title);
    if title.is_empty() {
        return None;
    }
    let media_type = if is_series(&title) {
        MediaType::Series
    } else {
        MediaType::Movie
    };
    Some(CatalogItem {
        id: ProviderMediaId {
            provider: ProviderKind::ToonWorld4All,
            value: url.path().to_string(),
        },
        year: site::year(&title),
        title,
        media_type,
        poster_url: poster,
        season_count: None,
    })
}

fn meta_image(base: &Url, post: &serde_json::Value) -> Option<String> {
    post.pointer("/meta/fifu_image_url")
        .and_then(|value| value.as_str())
        .filter(|url| !url.is_empty())
        .and_then(|url| image(base, url))
}

#[cfg(test)]
pub(super) fn api_search(base: &Url, json: &str) -> Result<Vec<CatalogItem>, ProviderError> {
    api_search_with_aliases(base, &[], json)
}

pub(super) fn api_search_with_aliases(
    base: &Url,
    aliases: &[Url],
    json: &str,
) -> Result<Vec<CatalogItem>, ProviderError> {
    let data: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| ProviderError::Parsing("Invalid WordPress search response".into()))?;
    let posts = data
        .as_array()
        .ok_or_else(|| ProviderError::Parsing("Invalid WordPress search results".into()))?;
    let mut seen = HashSet::new();
    Ok(posts
        .iter()
        .filter_map(|post| {
            let item = catalog_item(
                base,
                aliases,
                post.get("link")?.as_str()?,
                post.pointer("/title/rendered")?.as_str()?,
                meta_image(base, post),
            )?;
            seen.insert(item.id.value.clone()).then_some(item)
        })
        .collect())
}

/// HTML fallback for installations which disable the public WP REST API.
#[cfg(test)]
pub(super) fn html_search(base: &Url, html: &str) -> Vec<CatalogItem> {
    html_search_with_aliases(base, &[], html)
}

pub(super) fn html_search_with_aliases(
    base: &Url,
    aliases: &[Url],
    html: &str,
) -> Vec<CatalogItem> {
    let document = Html::parse_document(html);
    let articles = Selector::parse("article").unwrap();
    let links = Selector::parse("h2 a[href], h3 a[href], .entry-title a[href]").unwrap();
    let images = Selector::parse("img").unwrap();
    let mut seen = HashSet::new();
    document
        .select(&articles)
        .filter_map(|article| {
            let link = article.select(&links).next()?;
            let poster = article.select(&images).find_map(|img| {
                ["data-src", "data-lazy-src", "src"]
                    .iter()
                    .find_map(|attr| img.value().attr(attr))
                    .and_then(|src| image(base, src))
            });
            let item = catalog_item(
                base,
                aliases,
                link.value().attr("href")?,
                &site::text(Some(link))?,
                poster,
            )?;
            seen.insert(item.id.value.clone()).then_some(item)
        })
        .collect()
}

fn content_image(base: &Url, content: &str) -> Option<String> {
    let document = Html::parse_fragment(content);
    let images = Selector::parse("img[src], img[data-src]").unwrap();
    document.select(&images).find_map(|img| {
        img.value()
            .attr("data-src")
            .or_else(|| img.value().attr("src"))
            .and_then(|src| image(base, src))
    })
}

fn field(content: &str, label: &str) -> Option<String> {
    let document = Html::parse_fragment(content);
    let paragraphs = Selector::parse("p, li, h2, h3, h4").unwrap();
    document.select(&paragraphs).find_map(|paragraph| {
        let text = site::text(Some(paragraph))?;
        let start = text
            .to_ascii_lowercase()
            .find(&label.to_ascii_lowercase())?;
        // Do not treat a giant article/container as one metadata field.
        if start > 60 || text.len() > 900 {
            return None;
        }
        let value = text[start + label.len()..]
            .trim_start_matches([' ', ':', '-'])
            .trim();
        (!value.is_empty()).then(|| value.chars().take(500).collect())
    })
}

fn post_content(
    id: &str,
    origins: &Origins,
    raw_title: &str,
    content: &str,
    poster: Option<String>,
    excerpt: Option<String>,
) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
    let title = title(raw_title);
    if title.is_empty() {
        return Err(ProviderError::Parsing("Post title missing".into()));
    }
    let releases = post_releases(origins, &title, content);
    let media_type = if releases.iter().any(|release| release.season.is_some()) || is_series(&title)
    {
        MediaType::Series
    } else {
        MediaType::Movie
    };
    let genres = field(content, "Genre:")
        .map(|raw| {
            raw.split('|')
                .map(str::trim)
                .filter(|genre| !genre.is_empty() && genre.len() < 55)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let description = field(content, "Synopsis:").or(excerpt);
    let details = MediaDetails {
        id: ProviderMediaId {
            provider: ProviderKind::ToonWorld4All,
            value: id.to_string(),
        },
        year: site::year(&title),
        title,
        media_type,
        description,
        tagline: None,
        imdb_rating: field(content, "Ratings:"),
        director: field(content, "Directed By:"),
        stars: None,
        prints: field(content, "Quality:"),
        audios: field(content, "Language:"),
        poster_url: poster.or_else(|| content_image(&origins.site, content)),
        duration: field(content, "Running time:"),
        genres,
        seasons: site::seasons(&releases),
        dubs: Vec::new(),
    };
    Ok((details, releases))
}

pub(super) fn api_post(
    id: &str,
    origins: &Origins,
    json: &str,
) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
    let data: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| ProviderError::Parsing("Invalid WordPress post response".into()))?;
    let post = data
        .as_array()
        .and_then(|posts| posts.first())
        .ok_or(ProviderError::NotFound)?;
    // The requested slug must match the post. Never consume a different post from a redirect,
    // stale WP query, or a substituted JSON response.
    let url = post_url(
        &origins.site,
        &origins.site_aliases,
        post.get("link").and_then(|v| v.as_str()).unwrap_or(""),
    )
    .ok_or(ProviderError::NotFound)?;
    if url.path() != id {
        return Err(ProviderError::NotFound);
    }
    let content = post
        .pointer("/content/rendered")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProviderError::Parsing("WordPress post content missing".into()))?;
    let excerpt = post
        .pointer("/excerpt/rendered")
        .and_then(|v| v.as_str())
        .map(title);
    post_content(
        id,
        origins,
        post.pointer("/title/rendered")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        content,
        meta_image(&origins.site, post),
        excerpt,
    )
}

pub(super) fn html_post(
    id: &str,
    origins: &Origins,
    html: &str,
) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
    let document = Html::parse_document(html);
    let content = Selector::parse(".entry-content, .post-content, article .content").unwrap();
    let content = document
        .select(&content)
        .next()
        .ok_or_else(|| ProviderError::Parsing("Post content missing".into()))?;
    let heading = Selector::parse("h1.entry-title, article h1, h1").unwrap();
    let title = site::text(document.select(&heading).next())
        .or_else(|| site::attr(&document, "meta[property='og:title']", "content"))
        .ok_or_else(|| ProviderError::Parsing("Post title missing".into()))?;
    post_content(
        id,
        origins,
        &title,
        &content.html(),
        site::attr(&document, "meta[property='og:image']", "content")
            .and_then(|url| image(&origins.site, &url)),
        None,
    )
}

fn quality_line(raw: &str) -> Option<String> {
    let text = raw
        .trim()
        .trim_start_matches(['⇒', '→', '|', '•', '-', ' '])
        .trim();
    let lower = text.to_ascii_lowercase();
    if text.len() <= 70
        && ["2160p", "1080p", "720p", "480p", "360p"]
            .iter()
            .any(|quality| lower.starts_with(quality))
    {
        Some(text.to_string())
    } else {
        None
    }
}

fn episode_from_path(url: &Url) -> Option<(usize, usize)> {
    // Archive episode permalinks may end in '/', leaving an empty final path segment.
    let slug = url.path_segments()?.rfind(|segment| !segment.is_empty())?;
    let (season, episode) = slug.rsplit_once('-')?.1.split_once('x')?;
    let season = season.parse::<usize>().ok()?;
    let episode = episode.parse::<usize>().ok()?;
    (season > 0 && season <= 99 && episode > 0 && episode <= 2000).then_some((season, episode))
}

fn archive_origin(origins: &Origins, url: &Url) -> bool {
    if url.origin() == origins.archive.origin()
        || origins
            .archive_aliases
            .iter()
            .any(|alias| alias.origin() == url.origin())
    {
        return true;
    }
    // A new archive advertised by the current WordPress site may be on its archive subdomain.
    // Match the *entire* host, not a substring such as archive.site.example.evil.test.
    origins.site.host_str().is_some_and(|site_host| {
        let site_host = site_host.strip_prefix("www.").unwrap_or(site_host);
        url.host_str() == Some(format!("archive.{site_host}").as_str())
            && url.scheme() == origins.site.scheme()
    })
}

pub(super) fn is_archive_episode(origins: &Origins, url: &Url) -> bool {
    (archive_origin(origins, url)
        || url.origin() == origins.site.origin()
        || origins
            .site_aliases
            .iter()
            .any(|alias| alias.origin() == url.origin()))
        && url.path().starts_with("/episode/")
        && url.query().is_none()
}

pub(super) fn is_archive_movie(origins: &Origins, url: &Url) -> bool {
    archive_origin(origins, url) && url.path().starts_with("/movie/") && url.query().is_none()
}

fn is_archive_redirect(origins: &Origins, url: &Url) -> bool {
    archive_origin(origins, url) && url.path().starts_with("/redirect/")
}

fn is_worker_redirect(origins: &Origins, url: &Url) -> bool {
    (url.origin() == origins.worker.origin()
        || origins
            .worker_aliases
            .iter()
            .any(|alias| alias.origin() == url.origin()))
        && url.path() == "/redirect"
        && url
            .query_pairs()
            .any(|(key, val)| key == "data" && !val.is_empty())
}

/// Rebase only links with a recognized archive/site/worker origin and a known wrapper path.
/// Arbitrary third-party mirrors are left alone; they are never converted into archive links.
pub(super) fn canonical_link(origins: &Origins, url: &Url) -> Url {
    let (aliases, current) = if is_archive_movie(origins, url)
        || is_archive_episode(origins, url)
        || is_archive_redirect(origins, url)
    {
        if url.origin() == origins.site.origin()
            || origins
                .site_aliases
                .iter()
                .any(|alias| alias.origin() == url.origin())
        {
            (&origins.site_aliases, &origins.site)
        } else {
            (&origins.archive_aliases, &origins.archive)
        }
    } else if is_worker_redirect(origins, url) {
        (&origins.worker_aliases, &origins.worker)
    } else if drive::is_media_url(url)
        && origins
            .archive_aliases
            .iter()
            .any(|alias| alias.origin() == url.origin())
    {
        (&origins.archive_aliases, &origins.archive)
    } else if drive::is_media_url(url) || url.path().starts_with("/redirect/") {
        (&origins.site_aliases, &origins.site)
    } else {
        return url.clone();
    };
    for alias in aliases {
        if let Some(rebased) = site::rebase_url(alias, current, url) {
            return rebased;
        }
    }
    url.clone()
}

fn is_mirror(origins: &Origins, url: &Url) -> bool {
    if is_archive_redirect(origins, url)
        || is_worker_redirect(origins, url)
        || drive::is_media_url(url)
    {
        return true;
    }
    if url.origin() == origins.site.origin()
        || origins
            .site_aliases
            .iter()
            .any(|alias| alias.origin() == url.origin())
    {
        return url.path().starts_with("/redirect/");
    }
    drive::is_drive_page(url) || super::resolver::supported_host(url)
}

fn mirror_name(raw: &str, url: &Url) -> String {
    let text = raw.trim();
    let lower = text.to_ascii_lowercase();
    for (needle, label) in [
        ("hubcloud", "HubCloud"),
        ("filepress", "FilePress"),
        ("gdflix", "GDFlix"),
        ("mega", "MEGA"),
        ("appdrive", "AppDrive"),
        ("gdrive", "GDrive"),
        ("watch online", "Watch Online"),
    ] {
        if lower.contains(needle) && text.len() < 65 {
            return label.into();
        }
    }
    url.host_str().unwrap_or("Mirror").to_string()
}

fn release_stem(title: &str) -> &str {
    let lower = title.to_ascii_lowercase();
    let cut = [
        " 2160p", " 1080p", " 720p", " 480p", " 360p", " [2160p", " [1080p", " [720p", " [480p",
    ]
    .iter()
    .filter_map(|marker| lower.find(marker))
    .min()
    .unwrap_or(title.len());
    title[..cut].trim()
}

fn add_release(
    releases: &mut Vec<Release>,
    title: &str,
    quality: Option<&str>,
    episode: Option<(usize, usize)>,
    label: &str,
    url: &Url,
) {
    let title = release_stem(title);
    let name = match (episode, quality) {
        (Some((season, episode)), Some(quality)) => {
            format!("{title} S{season:02}E{episode:02} {quality}")
        }
        (Some((season, episode)), None) => format!("{title} S{season:02}E{episode:02}"),
        (None, Some(quality)) => format!("{title} {quality}"),
        (None, None) => format!("{title} Watch Online"),
    };
    let Some(mut release) =
        site::release(ProviderKind::ToonWorld4All, &name, url.as_str(), episode)
    else {
        return;
    };
    // The post title advertises *all* resolutions and codecs; the selected quality
    // heading, not the title, describes this particular release.
    release.quality = quality.and_then(site::quality);
    release.codec = quality.and_then(super::super::bdix::common::detect_codec);
    release.mirrors[0].label = label.to_string();
    if let Some(existing) = releases.iter_mut().find(|existing| {
        existing.filename == release.filename
            && existing.season == release.season
            && existing.episode == release.episode
    }) {
        if !existing
            .mirrors
            .iter()
            .any(|mirror| mirror.resolver_url == url.as_str())
        {
            existing.mirrors.extend(release.mirrors);
        }
    } else {
        releases.push(release);
    }
}

pub(super) fn post_releases(origins: &Origins, title: &str, content: &str) -> Vec<Release> {
    let document = Html::parse_fragment(content);
    let mut releases = Vec::new();
    let mut season = site::season_number(title).unwrap_or(1);
    let mut last_episode = None;
    let mut quality = None;
    for node in document.root_element().descendants() {
        if let Some(text) = node.value().as_text() {
            let text = text.trim();
            let lower = text.to_ascii_lowercase();
            if text.len() < 85 && lower.starts_with("season ") {
                if let Some(number) = site::season_number(text) {
                    season = number;
                }
            } else if text.len() < 85 && (lower.starts_with("episode ") || lower.starts_with('s')) {
                if let Some(marker) = site::episode_marker(text, season) {
                    last_episode = Some(marker);
                }
            }
            if let Some(label) = quality_line(text) {
                quality = Some(label);
            }
        }
        let Some(link) = ElementRef::wrap(node).filter(|node| node.value().name() == "a") else {
            continue;
        };
        let Some(url) = link
            .value()
            .attr("href")
            .and_then(|href| site::external_url(&origins.site, href))
        else {
            continue;
        };
        if is_archive_episode(origins, &url) {
            if let Some(episode) = episode_from_path(&url).or(last_episode) {
                let url = canonical_link(origins, &url);
                add_release(&mut releases, title, None, Some(episode), "Archive", &url);
            }
        } else if is_archive_movie(origins, &url) && !is_series(title) {
            // Recent movie posts contain only "Get Download Links"; the actual files
            // live on the separate archive and are loaded when this movie is selected.
            let url = canonical_link(origins, &url);
            add_release(&mut releases, title, Some("Archive"), None, "Archive", &url);
        } else if is_mirror(origins, &url) {
            let url = canonical_link(origins, &url);
            let label = mirror_name(&site::text(Some(link)).unwrap_or_default(), &url);
            let selected_quality = if label == "Watch Online" {
                Some("Watch Online")
            } else {
                quality.as_deref()
            };
            // A series' episode link page may contain direct media or mirrors as well.
            let episode = if is_series(title) { last_episode } else { None };
            add_release(
                &mut releases,
                title,
                selected_quality,
                episode,
                &label,
                &url,
            );
        }
    }
    releases
}

/// The archive page is a React app whose full file list ships as `window.__PROPS__`: every
/// encode (quality, codec, size) with its own mirror redirects. The visible HTML only renders
/// the selected encode, so reading the data gives all real qualities. Nothing is invented:
/// an encode without files yields no release.
fn props_releases(
    origins: &Origins,
    page_url: &Url,
    title: &str,
    episode: Option<(usize, usize)>,
    html: &str,
) -> Vec<Release> {
    const MARKER: &str = "window.__PROPS__ = ";
    let Some(start) = html.find(MARKER).map(|index| index + MARKER.len()) else {
        return Vec::new();
    };
    let Some(Ok(props)) = serde_json::Deserializer::from_str(&html[start..])
        .into_iter::<serde_json::Value>()
        .next()
    else {
        return Vec::new();
    };
    let Some(encodes) = props
        .pointer("/data/data/encodes")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut releases = Vec::new();
    for encode in encodes {
        let readable = encode.get("readable");
        let Some(codec) = readable
            .and_then(|value| value.get("codec"))
            .and_then(serde_json::Value::as_str)
            .filter(|text| site::quality(text).is_some())
        else {
            continue;
        };
        let label = match readable
            .and_then(|value| value.get("size"))
            .and_then(serde_json::Value::as_str)
        {
            Some(size) => format!("{codec} [{size}]"),
            None => codec.to_string(),
        };
        let files = encode.get("files").and_then(serde_json::Value::as_array);
        for file in files.into_iter().flatten() {
            let Some(url) = file
                .get("link")
                .and_then(serde_json::Value::as_str)
                .and_then(|href| site::external_url(page_url, href))
            else {
                continue;
            };
            if !(is_archive_redirect(origins, &url) || drive::is_media_url(&url)) {
                continue;
            }
            let host = file
                .get("host")
                .and_then(serde_json::Value::as_str)
                .filter(|host| !host.trim().is_empty())
                .unwrap_or("Archive");
            let url = canonical_link(origins, &url);
            add_release(&mut releases, title, Some(&label), episode, host, &url);
        }
    }
    releases
}

/// Only links actually present in the selected archive quality's Files section become releases.
/// Quality tabs with no files in the HTML are not invented as downloadable options.
pub(super) fn archive_releases(
    origins: &Origins,
    page_url: &Url,
    title: &str,
    episode: Option<(usize, usize)>,
    html: &str,
) -> Vec<Release> {
    let encoded = props_releases(origins, page_url, title, episode, html);
    if !encoded.is_empty() {
        return encoded;
    }
    let document = Html::parse_document(html);
    let mut releases = Vec::new();
    let mut quality = None;
    let mut mirror_label = "Archive".to_string();
    for node in document.root_element().descendants() {
        if let Some(text) = node.value().as_text() {
            let text = text.trim();
            if text.eq_ignore_ascii_case("AD SYSTEM") {
                quality = None;
            } else if let Some(label) = quality_line(text) {
                quality = Some(label);
            }
            if [
                "HubCloud",
                "Filepress",
                "GDFlix",
                "MEGA",
                "GDrive",
                "AppDrive",
                "Mirror",
                "GoFile",
                "PixelDrain",
            ]
            .iter()
            .any(|label| label.eq_ignore_ascii_case(text))
            {
                mirror_label = text.to_string();
            }
        }
        let Some(link) = ElementRef::wrap(node).filter(|node| node.value().name() == "a") else {
            continue;
        };
        let Some(url) = link
            .value()
            .attr("href")
            .and_then(|href| site::external_url(page_url, href))
        else {
            continue;
        };
        if is_archive_redirect(origins, &url) || drive::is_media_url(&url) {
            let url = canonical_link(origins, &url);
            add_release(
                &mut releases,
                title,
                quality.as_deref(),
                episode,
                &mirror_label,
                &url,
            );
        }
    }
    releases
}

/// The archive's redirect page explicitly displays its destination URL. Do not follow its
/// "Go to Destination" ad system; only extract the displayed URL and resolve it separately.
pub(super) fn destination_url(html: &str) -> Option<Url> {
    fn visible_url(text: &str) -> Option<Url> {
        let start = text.find("https://").or_else(|| text.find("http://"))?;
        let token = text[start..]
            .split_whitespace()
            .next()?
            .trim_end_matches(['`', '"', '\'', '<', '>', ')', ',']);
        site::safe_url(token).ok()
    }

    /// The redirect page renders `https://host/video/` and its file id as separate text
    /// nodes; the id completes the displayed destination.
    fn file_id(text: &str) -> Option<&str> {
        (text.len() >= 6
            && text.len() <= 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
        .then_some(text)
    }

    let document = Html::parse_document(html);
    let mut after_label = false;
    let mut pending: Option<Url> = None;
    for node in document.root_element().descendants() {
        let Some(text) = node.value().as_text() else {
            continue;
        };
        let trimmed = text.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("destination url") {
            after_label = true;
        }
        if !after_label || trimmed.is_empty() {
            continue;
        }
        if let Some(base) = &pending {
            if base.path().ends_with('/')
                && let Some(id) = file_id(trimmed)
                && let Ok(joined) = base.join(id)
            {
                return Some(joined);
            }
            return pending;
        }
        if lower.contains("24 hours system") || lower.contains("go to destination") {
            break;
        }
        if let Some(url) = visible_url(trimmed) {
            pending = Some(url);
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origins() -> Origins {
        use std::sync::LazyLock;
        static SITE: LazyLock<Url> =
            LazyLock::new(|| Url::parse("https://toonworld4all.me/").unwrap());
        static ARCHIVE: LazyLock<Url> =
            LazyLock::new(|| Url::parse("https://archive.toonworld4all.me/").unwrap());
        static WORKER: LazyLock<Url> =
            LazyLock::new(|| Url::parse("https://backend.tw4all.workers.dev/").unwrap());
        Origins {
            site: SITE.clone(),
            archive: ARCHIVE.clone(),
            worker: WORKER.clone(),
            site_aliases: vec![],
            archive_aliases: vec![],
            worker_aliases: vec![],
        }
    }

    #[test]
    fn wp_search_and_html_fallback_keep_movie_and_season_separate() {
        let base = origins().site;
        let json = r#"[{"link":"https://toonworld4all.me/tom-mars/","title":{"rendered":"Tom &amp; Jerry (2005)"},"meta":{"fifu_image_url":"https://img.example/mars.jpg"}},
            {"link":"https://toonworld4all.me/tom-season-1/","title":{"rendered":"The Tom and Jerry Show Season 1"}},
            {"link":"https://evil.example/post/","title":{"rendered":"Evil"}}]"#;
        let posts = api_search(&base, json).unwrap();
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].title, "Tom & Jerry (2005)");
        assert_eq!(posts[0].year.as_deref(), Some("2005"));
        assert_eq!(
            posts[0].poster_url.as_deref(),
            Some("https://img.example/mars.jpg")
        );
        assert_eq!(posts[1].media_type, MediaType::Series);
        let html = r#"<article><img src="https://img.example/poster.jpg"><h2 class="entry-title"><a href="/tom-season-1/">The Tom and Jerry Show Season 1</a></h2></article><nav><a href="/home/">Home</a></nav>"#;
        assert_eq!(html_search(&base, html)[0].id.value, "/tom-season-1/");
    }

    #[test]
    fn old_post_archive_and_worker_urls_rebase_only_from_known_origins() {
        let mut origins = origins();
        let old_site = origins.site.clone();
        let old_archive = origins.archive.clone();
        let old_worker = origins.worker.clone();
        origins.site = Url::parse("https://toonworld4all.new/").unwrap();
        origins.archive = Url::parse("https://archive.toonworld4all.new/").unwrap();
        origins.worker = Url::parse("https://worker.toonworld4all.new/").unwrap();
        origins.site_aliases.push(old_site);
        origins.archive_aliases.push(old_archive);
        origins.worker_aliases.push(old_worker);
        let json = serde_json::json!([{
            "link": "https://toonworld4all.me/film/",
            "title": {"rendered": "Film (2026)"},
            "content": {"rendered": "<p>480p SD</p><a href='https://archive.toonworld4all.me/movie/film'>Get Download Links</a><p>720p HD</p><a href='https://backend.tw4all.workers.dev/redirect?data=abc%2Bdef'>HubCloud</a><a href='https://evil.test/movie/film'>Bad link</a>"}
        }]).to_string();
        let (_, releases) = api_post("/film/", &origins, &json).unwrap();
        assert_eq!(releases.len(), 2);
        assert!(releases.iter().any(|r| {
            r.mirrors
                .iter()
                .any(|m| m.resolver_url == "https://archive.toonworld4all.new/movie/film")
        }));
        assert!(releases.iter().any(|r| {
            r.mirrors.iter().any(|m| {
                m.resolver_url == "https://worker.toonworld4all.new/redirect?data=abc%2Bdef"
            })
        }));
        assert!(!is_archive_movie(
            &origins,
            &Url::parse("https://archive.toonworld4all.new.evil.test/movie/film").unwrap()
        ));
        assert!(is_archive_movie(
            &origins,
            &Url::parse("https://archive.toonworld4all.new/movie/film").unwrap()
        ));
    }

    #[test]
    fn html_post_fallback_parses_the_article_not_navigation_links() {
        let origins = origins();
        let html = r#"<meta property='og:image' content='https://img.example/show.jpg'>
            <nav><a href='/other/'>Other show</a></nav><article><h1 class='entry-title'>Show Season 2</h1>
            <div class='entry-content'><p>Synopsis: A new adventure.</p><p>Episode 03</p>
            <a href='https://archive.toonworld4all.me/episode/show-2x3'>Watch/Download</a></div></article>"#;
        let (details, links) = html_post("/show-season-2/", &origins, html).unwrap();
        assert_eq!(
            details.poster_url.as_deref(),
            Some("https://img.example/show.jpg")
        );
        assert_eq!(details.seasons[0].number, 2);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].episode, Some(3));
    }

    #[test]
    fn wordpress_episode_links_use_their_real_season_and_episode() {
        let origins = origins();
        let json = r#"[{"link":"https://toonworld4all.me/jojo-season-3/","title":{"rendered":"JoJo&#8217;s Bizarre Adventure Season 3"},
            "meta":{"fifu_image_url":"https://img.example/jojo.jpg"},"content":{"rendered":"<p><strong>Synopsis:</strong> A family's adventures.</p><strong>SEASON 3</strong><div>Episode 01</div><a href='https://archive.toonworld4all.me/episode/jojos-bizarre-adventure-3x1'>Watch/Download</a><div>Episode 02</div><a href='https://archive.toonworld4all.me/episode/jojos-bizarre-adventure-3x2'>Watch/Download</a><a href='https://archive.toonworld4all.me/zip/jojo-s3'>Zip</a>"}}]"#;
        let (details, links) = api_post("/jojo-season-3/", &origins, json).unwrap();
        assert_eq!(details.media_type, MediaType::Series);
        assert_eq!(
            details.description.as_deref(),
            Some("A family's adventures.")
        );
        assert_eq!(details.seasons[0].number, 3);
        assert_eq!(details.seasons[0].episodes.len(), 2);
        assert_eq!(links[0].episode, Some(1));
        assert_eq!(links[1].episode, Some(2));
        assert_eq!(links[0].quality, None);
        assert_eq!(links[0].mirrors[0].label, "Archive");
    }

    #[test]
    fn archive_episode_permalinks_with_trailing_slashes_keep_episode_numbers() {
        let origins = origins();
        let releases = post_releases(
            &origins,
            "Show Season 2",
            concat!(
                "<a href='https://archive.toonworld4all.me/episode/show-2x3/'>Watch/Download</a>",
                "<a href='https://archive.toonworld4all.me/episode/show-2x4'>Watch/Download</a>",
            ),
        );
        assert_eq!(releases.len(), 2);
        assert_eq!(
            (releases[0].season, releases[0].episode),
            (Some(2), Some(3))
        );
        assert_eq!(
            (releases[1].season, releases[1].episode),
            (Some(2), Some(4))
        );
    }

    #[test]
    fn movie_variants_keep_their_quality_and_mirrors_without_zip_files() {
        let origins = origins();
        let json = r#"[{"link":"https://toonworld4all.me/tom-mars/","title":{"rendered":"Tom and Jerry Blast Off to Mars! (2005) 480p, 720p & 1080p HD | 10bit HEVC"},
            "content":{"rendered":"<p><strong>Synopsis:</strong> A trip to Mars.</p><p>⇒ <strong>480p SD</strong></p><p><a href='https://backend.tw4all.workers.dev/redirect?data=one'>HubCloud</a> | <a href='https://backend.tw4all.workers.dev/redirect?data=two'>FilePress</a></p><p>⇒ <strong>720p HD AAC 2.0</strong></p><p><a href='https://backend.tw4all.workers.dev/redirect?data=three'>HubCloud</a></p><p>⇒ <strong>720p HEVC 10bit Opus 2.0</strong></p><p><a href='https://backend.tw4all.workers.dev/redirect?data=four'>MEGA</a></p><a href='https://archive.toonworld4all.me/zip/tom'>Complete Zip</a>"}}]"#;
        let (details, releases) = api_post("/tom-mars/", &origins, json).unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(details.description.as_deref(), Some("A trip to Mars."));
        assert_eq!(releases.len(), 3);
        assert_eq!(releases[0].quality.as_deref(), Some("480p"));
        assert_eq!(releases[0].resolution_i64(), 480);
        assert_eq!(releases[0].codec, None); // HEVC in the post title is not the 480p codec
        assert!(!releases[0].filename.contains("1080p"));
        assert_eq!(releases[0].mirrors.len(), 2);
        assert_eq!(releases[1].quality.as_deref(), Some("720p"));
        assert_eq!(releases[2].quality.as_deref(), Some("720p"));
        assert_eq!(releases[2].codec.as_deref(), Some("HEVC"));
    }

    #[test]
    fn watch_online_does_not_inherit_the_post_titles_advertised_1080p() {
        let origins = origins();
        let links = post_releases(
            &origins,
            "Movie (2026) 480p 720p 1080p HEVC",
            "<a href='https://backend.tw4all.workers.dev/redirect?data=opaque'>Watch Online</a>",
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].quality, None);
        assert_eq!(links[0].resolution_u64(), 0);
        assert_eq!(links[0].resolution_i64(), -2);
    }

    #[test]
    fn archive_uses_only_mirrors_for_their_own_quality() {
        let origins = origins();
        let page_url = origins.archive.join("episode/show-1x1").unwrap();
        let html = r#"<h1>Show S01E01</h1><h3>480p x264</h3><h3>1080p HEVC 10bit</h3>
            <div>AD SYSTEM</div><h3>480p x264</h3><div>Files (2)</div>
            <h4>Filepress</h4><a href="/redirect/one">DOWNLOAD</a>
            <h4>GDFlix</h4><a href="/redirect/two">DOWNLOAD</a>
            <h3>1080p HEVC 10bit</h3><h4>HubCloud</h4>
            <a href="/redirect/three">DOWNLOAD</a><h4>Mirror</h4>
            <a href="/redirect/four">DOWNLOAD</a><a href="/zip/show-s1">ZIP</a>"#;
        let releases = archive_releases(
            &origins,
            &page_url,
            "Show Season 1 480p 720p 1080p HEVC",
            Some((1, 1)),
            html,
        );
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].quality.as_deref(), Some("480p"));
        assert_eq!(releases[0].codec.as_deref(), Some("x264"));
        assert_eq!(releases[0].mirrors.len(), 2);
        assert_eq!(releases[1].quality.as_deref(), Some("1080p"));
        assert_eq!(releases[1].mirrors[0].label, "HubCloud");
        assert_eq!(releases[1].mirrors[1].label, "Mirror");
        assert_eq!(releases[1].season, Some(1));
    }

    #[test]
    fn current_movie_posts_expand_archive_files_without_inventing_hidden_qualities() {
        let origins = origins();
        let json = serde_json::json!([{
            "link": "https://toonworld4all.me/toy-story-5-2026-bluray-multi-audio-hindi/",
            "title": {"rendered": "Toy Story 5 (2026) 480p, 720p & 2160p 4k"},
            "content": {"rendered": "<p>Single Download Links</p><a href='https://archive.toonworld4all.me/movie/toy-story-5-2026'>Get Download Links</a>"}
        }]).to_string();
        let (details, links) = api_post(
            "/toy-story-5-2026-bluray-multi-audio-hindi/",
            &origins,
            &json,
        )
        .unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].quality, None);
        assert!(links[0].filename.ends_with("Archive"));
        assert_eq!(links[0].mirrors[0].label, "Archive");
        let page = origins.archive.join("movie/toy-story-5-2026").unwrap();
        assert_eq!(links[0].direct_url(), Some(page.as_str()));
        let archive = r#"<h3>480p x264</h3><h3>720p HEVC</h3><h3>2160p HEVC</h3>
            <div>AD SYSTEM</div><h3>480p x264</h3><h4>HubCloud</h4>
            <a href='/redirect/movie-hub'>DOWNLOAD</a><h4>Filepress</h4>
            <a href='/redirect/movie-file'>DOWNLOAD</a>"#;
        let releases = archive_releases(&origins, &page, &details.title, None, archive);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].quality.as_deref(), Some("480p"));
        assert_eq!(releases[0].mirrors.len(), 2);
        assert!(releases[0].season.is_none());
        assert!(releases[0].episode.is_none());
        assert!(!releases[0].filename.contains("S00E00"));
    }

    #[test]
    fn redirect_page_requires_an_explicit_destination_label() {
        let html = r#"<a href="https://ad.example/">Ad</a><h2>Destination URL</h2>
            <code>https://hubcloud.ist/drive/abc</code><p>24 Hours System</p>"#;
        assert_eq!(
            destination_url(html).unwrap().host_str(),
            Some("hubcloud.ist")
        );
        assert_eq!(
            destination_url("<p>Destination URL: https://hubcloud.ist/drive/abc</p>")
                .unwrap()
                .path(),
            "/drive/abc"
        );
        assert!(destination_url("<a href='https://ad.example/'>Go to Destination</a>").is_none());
        assert!(
            destination_url("<p>Destination URL</p><code>http://evil.example/private</code>")
                .is_none()
        );
    }

    #[test]
    fn archive_props_expose_every_encode_with_its_own_mirrors() {
        let origins = origins();
        let page = Url::parse("https://archive.toonworld4all.me/movie/toy-story-5-2026").unwrap();
        let html = r#"<html><body><p>480p x264 494.85 MB</p>
            <script>window.__PAGE__ = "movie"; window.__PROPS__ = {"data":{"data":{"encodes":[
              {"resolution":"480p","readable":{"codec":"480p x264","size":"494.85 MB"},
               "files":[{"host":"HubCloud","link":"/redirect/aaa"},{"host":"GDFlix","link":"/redirect/bbb"}]},
              {"resolution":"1080p","readable":{"codec":"1080p HEVC 10bit [HQ]","size":"3.69 GB"},
               "files":[{"host":"HubCloud","link":"/redirect/ccc"}]},
              {"resolution":"2160p","readable":{"codec":"2160p HEVC 10bit [HQ]","size":"4.13 GB"},
               "files":[]},
              {"resolution":"720p","readable":{"codec":"720p x264","size":"978.29 MB"},
               "files":[{"host":"Evil","link":"https://evil.example/redirect/zzz"}]}
            ]}},"userSelectedSystem":"24hour"};</script></body></html>"#;
        let releases = archive_releases(&origins, &page, "Toy Story 5 (2026)", None, html);
        let found: Vec<(Option<&str>, usize, Option<u64>)> = releases
            .iter()
            .map(|r| (r.quality.as_deref(), r.mirrors.len(), r.size_bytes))
            .collect();
        // An encode without files or with only off-site links is never invented as a release.
        assert_eq!(found.len(), 2, "{releases:#?}");
        assert_eq!(found[0].0, Some("480p"));
        assert_eq!(found[0].1, 2);
        assert_eq!(found[1].0, Some("1080p"));
        assert_eq!(found[1].2, Some((3.69 * 1_073_741_824.0) as u64));
        assert_eq!(releases[0].mirrors[1].label, "GDFlix");
        assert!(releases[1].filename.contains("HEVC"));
    }

    #[test]
    fn destination_joins_the_displayed_base_and_file_id() {
        let split = r#"<p>Destination URL</p><div><span>https://hubcloud.ist/video/</span><span> 8bof3bxdbr6lnit </span></div>
            <p>24 Hours System</p>"#;
        assert_eq!(
            destination_url(split).unwrap().as_str(),
            "https://hubcloud.ist/video/8bof3bxdbr6lnit"
        );
        // A complete URL followed by unrelated words is not extended.
        let whole = "<p>Destination URL</p><span>https://hubcloud.ist/drive/abc</span><span>Recommended</span>";
        assert_eq!(destination_url(whole).unwrap().path(), "/drive/abc");
        // A base without a following id stays as displayed.
        let bare = "<p>Destination URL</p><span>https://hubcloud.ist/video/</span><span>24 Hours System</span>";
        assert_eq!(destination_url(bare).unwrap().path(), "/video/");
    }
}
