use std::sync::LazyLock;

use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

use crate::providers::models::{
    CatalogItem, MediaDetails, MediaType, ProviderError, ProviderKind, ProviderMediaId, Release,
};
use crate::providers::{drive, site};

static CARDS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("div.post-cards > article").unwrap());
static LINK: LazyLock<Selector> = LazyLock::new(|| Selector::parse("a[href]").unwrap());
static IMAGE: LazyLock<Selector> = LazyLock::new(|| Selector::parse("img").unwrap());
static SERIES_BUTTONS: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("a.maxbutton-episode-links, a.maxbutton-g-drive, a.maxbutton-af-download")
        .unwrap()
});
static MOVIE_BUTTONS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a.maxbutton-download-links").unwrap());
static EPISODE_HEADINGS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h2, h3, h4").unwrap());
static FILE_LINKS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a.maxbutton-1[href], a.maxbutton-5[href]").unwrap());

#[derive(Clone)]
pub(super) struct Button {
    pub url: Url,
    pub label: String,
    pub season: usize,
}

#[cfg(test)]
pub(super) fn search(base: &Url, html: &str) -> Vec<CatalogItem> {
    search_with_aliases(base, &[], html)
}

pub(super) fn search_with_aliases(base: &Url, aliases: &[Url], html: &str) -> Vec<CatalogItem> {
    let document = Html::parse_document(html);
    document
        .select(&CARDS)
        .filter_map(|card| {
            let anchor = card
                .select(&LINK)
                .find(|node| node.value().attr("href").is_some())?;
            let item = site::migrated_item_url(base, aliases, anchor.value().attr("href")?)?;
            let raw_title = anchor
                .value()
                .attr("title")
                .map(str::to_string)
                .or_else(|| site::text(Some(anchor)))?;
            let title = site::clean_title(&raw_title);
            if title.is_empty() || item.path() == "/" {
                return None;
            }
            let poster_url = card.select(&IMAGE).find_map(|img| {
                img.value()
                    .attr("data-src")
                    .or_else(|| img.value().attr("src"))
                    .and_then(|url| site::external_url(base, url))
                    .map(|url| url.to_string())
            });
            let is_series = site::season_number(&raw_title).is_some()
                || site::episode_marker(&raw_title, 1).is_some()
                || item.path().to_ascii_lowercase().contains("web-series");
            Some(CatalogItem {
                id: ProviderMediaId {
                    provider: ProviderKind::Moviesmod,
                    value: item.path().to_string(),
                },
                title,
                media_type: if is_series {
                    MediaType::Series
                } else {
                    MediaType::Movie
                },
                year: site::year(&raw_title),
                poster_url,
                season_count: None,
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn post(
    id: &str,
    base: &Url,
    html: &str,
) -> Result<(MediaDetails, Vec<Button>), ProviderError> {
    post_with_aliases(id, base, &[], html)
}

pub(super) fn post_with_aliases(
    id: &str,
    base: &Url,
    aliases: &[Url],
    html: &str,
) -> Result<(MediaDetails, Vec<Button>), ProviderError> {
    let document = Html::parse_document(html);
    let raw_title = site::attr(&document, "meta[property='og:title']", "content")
        .or_else(|| site::text(document.select(&Selector::parse("h1").unwrap()).next()))
        .ok_or_else(|| ProviderError::Parsing("Moviesmod post has no title".into()))?;
    let title = site::clean_title(&raw_title);
    let poster_url = site::attr(&document, "meta[property='og:image']", "content")
        .and_then(|url| site::external_url(base, &url))
        .map(|url| url.to_string());
    let description = site::text(
        document
            .select(&Selector::parse("div.imdbwp__teaser").unwrap())
            .next(),
    )
    .or_else(|| site::attr(&document, "meta[name='description']", "content"));
    let is_series = document.select(&SERIES_BUTTONS).next().is_some()
        || site::season_number(&raw_title).is_some()
        || document
            .select(
                &Selector::parse("div.thecontent h2, div.thecontent h3, div.thecontent h4")
                    .unwrap(),
            )
            .filter_map(|heading| site::text(Some(heading)))
            .any(|heading| site::season_number(&heading).is_some());
    let default_season = site::season_number(&raw_title).unwrap_or(1);
    let buttons = document
        .select(if is_series {
            &SERIES_BUTTONS
        } else {
            &MOVIE_BUTTONS
        })
        .filter_map(|button| {
            let href = button.value().attr("href")?;
            let href = site::provider_link(base, aliases, href)?;
            let url = drive::unwrap_url(&href).unwrap_or(href);
            let nearby = nearby_label(button);
            Some(Button {
                url,
                season: site::season_number(&nearby).unwrap_or(default_season),
                label: nearby,
            })
        })
        .collect();
    let genres = document
        .select(&Selector::parse("a[rel='category tag'], .genres a").unwrap())
        .filter_map(|node| site::text(Some(node)))
        .collect();
    let details = MediaDetails {
        id: ProviderMediaId {
            provider: ProviderKind::Moviesmod,
            value: id.to_string(),
        },
        title,
        media_type: if is_series {
            MediaType::Series
        } else {
            MediaType::Movie
        },
        year: site::year(&raw_title),
        description,
        tagline: None,
        imdb_rating: None,
        director: None,
        stars: None,
        prints: None,
        audios: None,
        poster_url,
        duration: None,
        genres,
        seasons: vec![],
        dubs: vec![],
    };
    Ok((details, buttons))
}

fn nearby_label(button: ElementRef<'_>) -> String {
    let direct = site::text(Some(button)).unwrap_or_default();
    let parent = button.parent().and_then(ElementRef::wrap);
    let shared_parent = parent.is_some_and(|node| node.select(&LINK).nth(1).is_some());
    let mut label = if shared_parent {
        site::preceding_quality(button)
            .map(|quality| format!("{quality} {direct}"))
            .unwrap_or(direct)
    } else {
        parent
            .and_then(|node| site::text(Some(node)))
            .filter(|text| text.len() > direct.len() + 5 && text.len() < 160)
            .unwrap_or(direct)
    };
    let mut node = parent.and_then(site::previous_element);
    for _ in 0..5 {
        let Some(current) = node else { break };
        if let Some(text) = site::text(Some(current)) {
            if site::season_number(&text).is_some() {
                return format!("{text} {label}").chars().take(160).collect();
            }
            if site::quality(&label).is_none() && site::quality(&text).is_some() {
                label = format!("{text} {label}");
            }
        }
        node = site::previous_element(current);
    }
    label.chars().take(160).collect()
}

// SEO post titles advertise multiple resolutions and file sizes. Read release metadata only
// from the selected button (or its intermediate page), then prefix a neutral display title.
fn release_title(title: &str) -> &str {
    let lower = title.to_ascii_lowercase();
    let cut = [" 2160p", " 1080p", " 720p", " 480p", " 360p", " 4k"]
        .iter()
        .filter_map(|marker| lower.find(marker))
        .min()
        .unwrap_or(title.len());
    title[..cut].trim()
}

fn named_release(
    title: &str,
    label: &str,
    url: &Url,
    episode: Option<(usize, usize)>,
) -> Option<Release> {
    let mut release = site::release(ProviderKind::Moviesmod, label, url.as_str(), episode)?;
    if release.language.is_none() {
        release.language = crate::providers::bdix::common::detect_audio_language(title);
    }
    release.filename = format!("{} {}", release_title(title), release.filename);
    Some(release)
}

/// Parse a Moviesmod intermediate page (movie download buttons or series h3/h4 episode links).
/// A direct drive page does not need to be fetched until the user selects the release.
pub(super) fn links(
    button: &Button,
    title: &str,
    is_series: bool,
    page: Option<(&Url, &str)>,
) -> Option<Vec<Release>> {
    let episode = is_series
        .then(|| site::episode_marker(&button.label, button.season).unwrap_or((button.season, 1)));
    let Some((base, html)) = page else {
        return named_release(title, &button.label, &button.url, episode)
            .map(|release| vec![release]);
    };
    if drive::is_drive_page(base) {
        return named_release(title, &button.label, base, episode).map(|release| vec![release]);
    }
    let document = Html::parse_document(html);
    if is_series {
        let mut releases = Vec::new();
        let mut season = button.season;
        let mut ordinal = 0;
        for heading in document.select(&EPISODE_HEADINGS) {
            let label = site::text(Some(heading)).unwrap_or_default();
            if let Some(season_number) = site::season_number(&label) {
                season = season_number;
            }
            let Some(link) = heading.select(&LINK).next() else {
                continue;
            };
            let Some(href) = link
                .value()
                .attr("href")
                .and_then(|h| site::external_url(base, h))
            else {
                continue;
            };
            let url = drive::unwrap_url(&href).unwrap_or(href);
            ordinal += 1;
            let (se, ep) = site::episode_marker(&label, season).unwrap_or((season, ordinal));
            let metadata = if site::quality(&button.label).is_some() {
                button.label.as_str()
            } else {
                label.as_str()
            };
            if let Some(mut release) = named_release(title, metadata, &url, Some((se, ep))) {
                release.filename = format!(
                    "{} S{se:02}E{ep:02} {} {label}",
                    release_title(title),
                    button.label
                );
                release.mirrors[0].label = "Driveleech".into();
                releases.push(release);
            }
        }
        Some(releases)
    } else {
        let page_title = site::text(document.select(&Selector::parse("h1, h2").unwrap()).next())
            .unwrap_or_default();
        let label = if site::quality(&button.label).is_some() {
            button.label.as_str()
        } else if !page_title.is_empty() {
            page_title.as_str()
        } else {
            button.label.as_str()
        };
        let mut release = None;
        for link in document.select(&FILE_LINKS) {
            let Some(href) = link
                .value()
                .attr("href")
                .and_then(|h| site::external_url(base, h))
            else {
                continue;
            };
            let url = drive::unwrap_url(&href).unwrap_or(href);
            if release.is_none() {
                release = named_release(title, label, &url, None);
            } else if let Some(ref mut release) = release {
                if let Some(mirror) = named_release(title, label, &url, None)
                    .and_then(|r| r.mirrors.into_iter().next())
                    && !release
                        .mirrors
                        .iter()
                        .any(|m| m.resolver_url == mirror.resolver_url)
                {
                    release.mirrors.push(mirror);
                }
            }
        }
        // Some versions link directly to Driveleech instead of a maxbutton page.
        if release.is_none() && drive::is_drive_page(&button.url) {
            release = named_release(title, label, &button.url, None);
        }
        release.map(|r| vec![r])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_posts_and_ignore_offsite_cards() {
        let base = site::base_url("https://moviesmod.example/").unwrap();
        let html = r#"<div class="post-cards">
            <article><a title="Download Dune (2021) 2160p" href="/dune/"><div><img src="/dune.webp"></div></a></article>
            <article><a title="Download Show Season 3" href="/show-season-3/"><img src="/s.webp"></a></article>
            <article><a title="Offsite" href="https://evil.example/post"></a></article>
        </div>"#;
        let cards = search(&base, html);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id.value, "/dune/");
        assert_eq!(cards[0].year.as_deref(), Some("2021"));
        assert_eq!(cards[1].media_type, MediaType::Series);
    }

    #[test]
    fn current_modpro_buttons_keep_each_releases_own_quality_and_size() {
        let base = site::base_url("https://moviesmod.ai.in/").unwrap();
        let post_html = r#"<meta property='og:title' content='Download Mile End Kicks (2026) 1080p [2.4GB]'>
            <div class='thecontent'><p>This movie takes place during the holiday season.</p>
            <h4>Download Mile End Kicks (2026) 480p x264 [370MB]</h4>
            <p><a class='maxbutton-download-links' href='https://links.modpro.blog/archives/155114'>Download Links</a></p>
            <h4>Download Mile End Kicks (2026) 720p HEVC [650MB]</h4>
            <p><a class='maxbutton-download-links' href='https://links.modpro.blog/archives/155115'>Download Links</a></p>
            </div>"#;
        let (details, buttons) = post("/download-mile-end-kicks-2026/", &base, post_html).unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(buttons.len(), 2);
        let modpro = base
            .join("https://links.modpro.blog/archives/155114")
            .unwrap();
        let child_html = r#"<h1>Modpro.blog</h1><p>Choose any Download Server</p>
            <a class='maxbutton-1' href='https://en.thenaukriadda.in/?sid=first'>Fast Server (G-Drive)</a>
            <a class='maxbutton-5' href='https://en.thenaukriadda.in/?sid=second'>Google Drive (Server 2)</a>"#;
        let low = links(
            &buttons[0],
            &details.title,
            false,
            Some((&modpro, child_html)),
        )
        .unwrap();
        let high = links(
            &buttons[1],
            &details.title,
            false,
            Some((&modpro, child_html)),
        )
        .unwrap();
        assert_eq!(low[0].quality.as_deref(), Some("480p"));
        assert_eq!(high[0].quality.as_deref(), Some("720p"));
        assert_eq!(low[0].size_bytes, Some(370 * 1_048_576));
        assert_eq!(high[0].size_bytes, Some(650 * 1_048_576));
        assert_eq!(low[0].mirrors.len(), 2);
    }

    #[test]
    fn current_series_links_ignore_other_qualities_in_the_post_title() {
        let button = Button {
            url: Url::parse("https://episodes.modpro.blog/archives/75150").unwrap(),
            label: "Season 1 480p x264 [70MB]".into(),
            season: 1,
        };
        let html = r#"<a href='/batch.zip'>All Episodes Batch</a>
            <h3><a href='https://cloud.unblockedgames.world/?sid=one'>Episode 1</a></h3>
            <h3><a href='https://cloud.unblockedgames.world/?sid=two'>Episode 2</a></h3>"#;
        let releases = links(
            &button,
            "Avatar Season 1 2160p",
            true,
            Some((&button.url, html)),
        )
        .unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].episode, Some(1));
        assert_eq!(releases[1].episode, Some(2));
        assert_eq!(releases[0].quality.as_deref(), Some("480p"));
        assert_eq!(releases[0].size_bytes, Some(70 * 1_048_576));
        assert!(!releases[0].filename.contains("2160p"));
    }

    #[test]
    fn parses_movie_download_page_and_series_episode_groups() {
        let base = site::base_url("https://moviesmod.example/").unwrap();
        let movie = r#"<meta property="og:title" content="Download Dune (2021)">
            <meta property="og:image" content="/dune.jpg"><div class="thecontent">
            <p><strong>1080p 2 GB</strong></p><a class="maxbutton-download-links" href="/links/">Download Links</a></div>"#;
        let (details, buttons) = post("/dune/", &base, movie).unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(
            details.poster_url.as_deref(),
            Some("https://moviesmod.example/dune.jpg")
        );
        assert_eq!(buttons.len(), 1);
        let shared = r#"<meta property="og:title" content="Download Dune (2021)"><div class="thecontent"><p>
            <a class="maxbutton-download-links" href="/links/">1080p</a>
            <a class="maxbutton-download-links" href="/other/">2160p</a></p></div>"#;
        let (_, distinct_buttons) = post("/dune/", &base, shared).unwrap();
        assert_eq!(
            distinct_buttons
                .iter()
                .map(|b| site::quality(&b.label))
                .collect::<Vec<_>>(),
            vec![Some("1080p".into()), Some("2160p".into())]
        );
        let sibling_labels = r#"<meta property="og:title" content="Download Dune (2021)"><div class="thecontent"><p>
            <strong>1080p</strong><a class="maxbutton-download-links" href="/links/">Download</a>
            <strong>2160p</strong><a class="maxbutton-download-links" href="/other/">Download</a></p></div>"#;
        let (_, labels) = post("/dune/", &base, sibling_labels).unwrap();
        assert_eq!(site::quality(&labels[0].label).as_deref(), Some("1080p"));
        assert_eq!(site::quality(&labels[1].label).as_deref(), Some("2160p"));
        let child_url = base.join("links/").unwrap();
        let child_html = r#"<h2>Dune.1080p.HEVC 2 GB</h2>
            <a class="maxbutton-1" href="https://driveleech.example/file/one">Server 1</a>
            <a class="maxbutton-5" href="https://driveseed.example/file/two">Server 2</a>"#;
        let releases = links(
            &buttons[0],
            &details.title,
            false,
            Some((&child_url, child_html)),
        )
        .unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].mirrors.len(), 2);
        assert_eq!(releases[0].quality.as_deref(), Some("1080p"));

        let series = r#"<meta property="og:title" content="Download Show Season 2"><div class="thecontent">
            <h3>Season 2</h3><p><a class="maxbutton-episode-links" href="/episodes/">1080p</a></p>
            </div>"#;
        let (details, buttons) = post("/show-season-2/", &base, series).unwrap();
        assert_eq!(details.media_type, MediaType::Series);
        assert_eq!(buttons[0].season, 2);
        let child_url = base.join("episodes/").unwrap();
        let child_html = r#"<h3>Episode 1 <a href="https://driveseed.example/file/one">Server</a></h3>
            <h4>Episode 2 <a href="https://driveseed.example/file/two">Server</a></h4>"#;
        let releases = links(
            &buttons[0],
            &details.title,
            true,
            Some((&child_url, child_html)),
        )
        .unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].season, Some(2));
        assert_eq!(releases[1].episode, Some(2));
        assert_eq!(site::seasons(&releases)[0].episodes.len(), 2);

        let direct = Button {
            url: Url::parse("https://driveleech.example/file/episode").unwrap(),
            label: "Season 2 Episode 8 1080p".into(),
            season: 2,
        };
        let releases = links(&direct, "Show", true, None).unwrap();
        assert_eq!(releases[0].episode, Some(8));
    }
}
