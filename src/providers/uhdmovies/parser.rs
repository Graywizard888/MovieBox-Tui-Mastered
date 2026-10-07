use std::sync::LazyLock;

use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

use crate::providers::models::{
    CatalogItem, MediaDetails, MediaType, ProviderError, ProviderKind, ProviderMediaId, Release,
};
use crate::providers::site;

static CARDS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article.gridlove-post").unwrap());
static CARD_LINK: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".entry-image a[href], a[href*='/download-']").unwrap());
static CARD_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1.sanket, .box-inner-p a, h2 a, h3 a, h2, h3").unwrap());
static CARD_IMAGE: LazyLock<Selector> = LazyLock::new(|| Selector::parse("img").unwrap());
static HEADLINE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1.entry-title, h1").unwrap());
// Episodes in the referenced extension are often ordinary "Episode N" anchors rather than
// maxbuttons. Only links to supported landing/Drive hosts are retained below.
static BUTTONS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".entry-content a[href]").unwrap());
static SIBLING_BUTTONS: LazyLock<Selector> = LazyLock::new(|| Selector::parse("a[href]").unwrap());
static CATEGORIES: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".entry-category a").unwrap());

#[cfg(test)]
pub(super) fn search(base: &Url, html: &str) -> Vec<CatalogItem> {
    search_with_aliases(base, &[], html)
}

pub(super) fn search_with_aliases(base: &Url, aliases: &[Url], html: &str) -> Vec<CatalogItem> {
    let document = Html::parse_document(html);
    document
        .select(&CARDS)
        .filter_map(|card| {
            let link = card.select(&CARD_LINK).next()?;
            let item = site::migrated_item_url(base, aliases, link.value().attr("href")?)?;
            if item.path() == "/" || item.path().starts_with("/page/") {
                return None;
            }
            let title_node = card.select(&CARD_TITLE).next();
            let raw = title_node
                .and_then(|node| node.value().attr("title").map(str::to_string))
                .or_else(|| site::text(title_node))
                .or_else(|| link.value().attr("title").map(str::to_string))?;
            let title = site::clean_title(&raw);
            if title.is_empty() {
                return None;
            }
            let poster_url = card.select(&CARD_IMAGE).find_map(|img| {
                img.value()
                    .attr("data-src")
                    .or_else(|| img.value().attr("src"))
                    .and_then(|href| site::external_url(base, href))
                    .map(|url| url.to_string())
            });
            let is_series = site::season_number(&raw).is_some()
                || site::episode_marker(&raw, 1).is_some()
                || item.path().to_ascii_lowercase().contains("season");
            Some(CatalogItem {
                id: ProviderMediaId {
                    provider: ProviderKind::UhdMovies,
                    value: item.path().to_string(),
                },
                title,
                media_type: if is_series {
                    MediaType::Series
                } else {
                    MediaType::Movie
                },
                year: site::year(&raw),
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
) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
    post_with_aliases(id, base, &[], html)
}

pub(super) fn post_with_aliases(
    id: &str,
    base: &Url,
    aliases: &[Url],
    html: &str,
) -> Result<(MediaDetails, Vec<Release>), ProviderError> {
    let document = Html::parse_document(html);
    let raw_title = site::text(document.select(&HEADLINE).next())
        .or_else(|| site::attr(&document, "meta[property='og:title']", "content"))
        .ok_or_else(|| ProviderError::Parsing("UHDMovies post has no title".into()))?;
    let title = site::clean_title(&raw_title);
    let poster_url = site::attr(&document, ".entry-content img", "data-src")
        .or_else(|| site::attr(&document, ".entry-content img", "src"))
        .or_else(|| site::attr(&document, "meta[property='og:image']", "content"))
        .and_then(|href| site::external_url(base, &href))
        .map(|url| url.to_string());
    let description = site::attr(&document, "meta[name='description']", "content").or_else(|| {
        site::text(
            document
                .select(&Selector::parse(".entry-content > p").unwrap())
                .next(),
        )
    });
    let genres: Vec<String> = document
        .select(&CATEGORIES)
        .filter_map(|node| site::text(Some(node)))
        .collect();

    // Only download buttons inside the post. Posts carry SEO paragraphs whose inline "4k",
    // "2160p HEVC", "1080p UHD" or "3D Movies" links also point at the link gate but land on
    // category pages. When the post has real maxbutton anchors, nothing else is a release.
    let anchors: Vec<ElementRef<'_>> = document.select(&BUTTONS).collect();
    let has_maxbuttons = anchors.iter().copied().any(is_maxbutton);
    let links: Vec<(String, String, bool)> = anchors
        .into_iter()
        .filter(|button| !has_maxbuttons || is_maxbutton(*button))
        .filter_map(|button| {
            let href = button.value().attr("href")?;
            let url = site::provider_link(base, aliases, href)?;
            let host = url.host_str()?.to_ascii_lowercase();
            if !url.query_pairs().any(|(key, _)| key == "sid")
                && !host.contains("driveseed.")
                && !host.contains("driveleech.")
            {
                return None;
            }
            let (label, is_zip) = if is_maxbutton(button) {
                let text = site::text(Some(button))
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let class = button
                    .value()
                    .attr("class")
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let zip = class.contains("zip") || text.contains("zip") || text.contains("pack");
                (
                    maxbutton_label(button).unwrap_or_else(|| label_for(button)),
                    zip,
                )
            } else {
                let label = label_for(button);
                let zip = label.to_ascii_lowercase().contains("zip");
                (label, zip)
            };
            Some((label, url.to_string(), is_zip))
        })
        .collect();
    let default_season = site::season_number(&raw_title).unwrap_or(1);
    let is_series = site::season_number(&raw_title).is_some()
        || links
            .iter()
            .any(|(label, _, _)| site::episode_marker(label, default_season).is_some());

    let mut releases: Vec<Release> = Vec::new();
    for (label, href, is_zip) in links {
        if is_series && is_zip {
            continue; // full-season archives are not playable episodes
        }
        let episode = if is_series {
            site::episode_marker(&label, default_season).or(Some((default_season, 1)))
        } else {
            None
        };
        if let Some(mut release) = site::release(ProviderKind::UhdMovies, &label, &href, episode) {
            release.mirrors[0].label = "G-Drive".to_string();
            if let Some(existing) = releases.iter_mut().find(|r| {
                r.filename == release.filename
                    && r.season == release.season
                    && r.episode == release.episode
            }) {
                for mirror in release.mirrors {
                    if !existing
                        .mirrors
                        .iter()
                        .any(|current| current.resolver_url == mirror.resolver_url)
                    {
                        existing.mirrors.push(mirror);
                    }
                }
            } else {
                releases.push(release);
            }
        }
    }
    let seasons = site::seasons(&releases);
    let details = MediaDetails {
        id: ProviderMediaId {
            provider: ProviderKind::UhdMovies,
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
        seasons,
        dubs: vec![],
    };
    Ok((details, releases))
}

fn is_maxbutton(anchor: ElementRef<'_>) -> bool {
    anchor
        .value()
        .attr("class")
        .is_some_and(|class| class.to_ascii_lowercase().contains("maxbutton"))
}

/// Current posts put each release's description ("Film (2010) 1080p 10bit … [7.07GB]") in the
/// paragraph *before* the paragraph holding its "Download (G-Drive)" or "Episode N" buttons.
/// Returns `None` when the button carries its own description (older markup).
fn maxbutton_label(button: ElementRef<'_>) -> Option<String> {
    if site::preceding_quality(button).is_some() {
        return None;
    }
    let button_text = site::text(Some(button)).unwrap_or_default();
    let mut block = button;
    while let Some(parent) = block.parent().and_then(ElementRef::wrap) {
        if parent.value().name() == "body"
            || parent
                .value()
                .classes()
                .any(|class| class == "entry-content")
        {
            break;
        }
        block = parent;
    }
    let mut description = None;
    let mut sibling = site::previous_element(block);
    for _ in 0..12 {
        let Some(node) = sibling else { break };
        sibling = site::previous_element(node);
        // The previous release's buttons mean this release has no description of its own.
        if node.select(&SIBLING_BUTTONS).any(is_maxbutton) {
            break;
        }
        let Some(text) = site::text(Some(node)) else {
            continue;
        };
        if text.len() <= 400
            && (site::quality(&text).is_some()
                || crate::providers::models::parse_size_bytes(&text).is_some()
                || site::episode_marker(&text, 1).is_some())
        {
            description = Some(text);
            break;
        }
    }
    let description = description?;
    let mut label = if site::episode_marker(&button_text, 1).is_some() {
        format!("{description} {button_text}")
    } else {
        description
    };
    if site::season_number(&label).is_none()
        && let Some(season) = season_heading(block)
    {
        label = format!("Season {season} {label}");
    }
    Some(label.chars().take(240).collect())
}

/// `<pre>SEASON 2</pre>` style headings precede the first release group of a season.
fn season_heading(block: ElementRef<'_>) -> Option<usize> {
    let mut sibling = site::previous_element(block);
    for _ in 0..80 {
        let node = sibling?;
        sibling = site::previous_element(node);
        if matches!(node.value().name(), "pre" | "h2" | "h3" | "h4")
            && let Some(text) = site::text(Some(node))
            && text.len() < 60
            && let Some(season) = site::season_number(&text)
        {
            return Some(season);
        }
    }
    None
}

fn label_for(button: ElementRef<'_>) -> String {
    let button_text = site::text(Some(button)).unwrap_or_default();
    let mut label = button
        .value()
        .attr("title")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if button_text.is_empty() {
                "Download".into()
            } else {
                button_text.clone()
            }
        });
    // A movie's quality is usually in a <strong> preceding a maxbutton. Series often have
    // a separate <pre>Season N</pre> followed by plain "Episode N" links.
    let mut node = button.parent().and_then(ElementRef::wrap);
    let mut season_hint = None;
    let mut found_context = false;
    for _ in 0..7 {
        let Some(current) = node else { break };
        if let Some(text) = site::text(Some(current))
            && text.len() < 200
        {
            if let Some(season) = site::season_number(&text)
                && season_hint.is_none()
            {
                season_hint = Some(season);
            }
            if !found_context {
                let strong = Selector::parse("strong").unwrap();
                let strong_text = site::text(current.select(&strong).next());
                let candidate = strong_text
                    .as_deref()
                    .filter(|text| {
                        site::quality(text).is_some() || site::episode_marker(text, 1).is_some()
                    })
                    .unwrap_or(text.as_str());
                if candidate.len() > button_text.len() + 4
                    || site::quality(candidate).is_some()
                    || site::episode_marker(candidate, 1).is_some()
                {
                    label = candidate.to_string();
                    found_context = true;
                }
            }
        }
        node = site::previous_element(current);
    }
    // Some posts put several "Episode N" anchors in one paragraph. Its combined text must
    // not override the episode number on each individual anchor.
    let shared_parent = button
        .parent()
        .and_then(ElementRef::wrap)
        .is_some_and(|node| node.select(&SIBLING_BUTTONS).nth(1).is_some());
    let preceding_quality = site::preceding_quality(button);
    if site::episode_marker(&button_text, 1).is_some() {
        label = preceding_quality
            .map(|quality| format!("{quality} {button_text}"))
            .unwrap_or(button_text);
    } else if let Some(quality) = preceding_quality {
        label = quality;
    } else if shared_parent && site::quality(&button_text).is_some() {
        label = button_text;
    }
    if let Some(season) = season_hint
        && site::season_number(&label).is_none()
        && site::episode_marker(&label, season).is_some()
    {
        label = format!("Season {season} {label}");
    }
    label.chars().take(180).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_reads_gridlove_cards_and_ignores_external_navigation() {
        let base = site::base_url("https://uhdmovies.example/").unwrap();
        let html = r#"
          <article class="gridlove-post"><div class="entry-image"><a href="/download-arrival-2016/"><img data-src="/poster.webp"></a></div>
            <h1 class="sanket">Download Arrival (2016) 2160p Dual Audio</h1></article>
          <article class="gridlove-post"><div class="entry-image"><a href="/download-show-season-2/"></a></div>
            <div class="box-inner-p"><a title="Download Show Season 2">Show</a></div></article>
          <article class="gridlove-post"><a href="https://elsewhere.example/download-bad/">Bad</a></article>
        "#;
        let items = search(&base, html);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id.value, "/download-arrival-2016/");
        assert_eq!(
            items[0].poster_url.as_deref(),
            Some("https://uhdmovies.example/poster.webp")
        );
        assert_eq!(items[1].media_type, MediaType::Series);
    }

    #[test]
    fn parses_movie_and_multiple_qualities_per_episode() {
        let base = site::base_url("https://uhdmovies.example/").unwrap();
        let movie = r#"<h1 class="entry-title">Download Arrival (2016) 2160p</h1>
            <meta property="og:image" content="/cover.jpg"><div class="entry-content">
            <p><strong>Arrival.2160p.HEVC [8 GB]</strong><a class="maxbutton-download-g-drive" href="https://cloud.unblockedgames.world/?sid=a">G-Drive</a><a class="maxbutton-download-g-drive" href="https://cloud.unblockedgames.world/?sid=backup">Mirror</a></p>
            <p><strong>Arrival.1080p [2 GB]</strong><a class="maxbutton-download-g-drive" href="https://cloud.unblockedgames.world/?sid=b">G-Drive</a></p>
            <a href="https://uhdmovies.example/movies/">2160p HEVC</a></div>"#;
        let (details, releases) = post("/download-arrival-2016/", &base, movie).unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(
            details.poster_url.as_deref(),
            Some("https://uhdmovies.example/cover.jpg")
        );
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].mirrors.len(), 2);
        assert_eq!(releases[0].quality.as_deref(), Some("2160p"));
        assert_eq!(releases[0].size_bytes, Some(8 * 1024 * 1024 * 1024));
        assert!(!releases[0].mirrors[0].direct_file);

        let shared = r#"<h1 class="entry-title">Download Arrival (2016)</h1><div class="entry-content"><p>
            <a href="https://cloud.unblockedgames.world/?sid=low">1080p</a>
            <a href="https://cloud.unblockedgames.world/?sid=high">2160p</a></p></div>"#;
        let (_, distinct) = post("/download-arrival/", &base, shared).unwrap();
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct[0].quality.as_deref(), Some("1080p"));
        assert_eq!(distinct[1].quality.as_deref(), Some("2160p"));

        let siblings = r#"<h1 class="entry-title">Download Arrival (2016)</h1><div class="entry-content"><p>
            <strong>1080p</strong><a href="https://cloud.unblockedgames.world/?sid=low">Download</a>
            <strong>2160p</strong><a href="https://cloud.unblockedgames.world/?sid=high">Download</a></p></div>"#;
        let (_, distinct) = post("/download-arrival/", &base, siblings).unwrap();
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct[0].quality.as_deref(), Some("1080p"));
        assert_eq!(distinct[1].quality.as_deref(), Some("2160p"));

        let series = r#"<h1 class="entry-title">Download Lanterns Season 2</h1><div class="entry-content">
            <p><strong>Lanterns S02E03 2160p 5 GB</strong><a class="maxbutton-gdrive-episode" href="https://cloud.unblockedgames.world/?sid=one">Download</a></p>
            <p><strong>Lanterns S02E03 1080p 2 GB</strong><a class="maxbutton-gdrive-episode" href="https://cloud.unblockedgames.world/?sid=two">Download</a></p>
            <p><strong>Lanterns S02E04 1080p</strong><a class="maxbutton-gdrive-episode" href="https://cloud.unblockedgames.world/?sid=three">Download</a></p></div>"#;
        let (details, releases) = post("/download-lanterns-season-2/", &base, series).unwrap();
        assert_eq!(details.media_type, MediaType::Series);
        assert_eq!(details.seasons[0].number, 2);
        assert_eq!(details.seasons[0].episodes.len(), 2);
        assert_eq!(releases.iter().filter(|r| r.episode == Some(3)).count(), 2);

        // Current posts use en.thenaukriadda.in sid links. The 1080p UHD label is
        // still 1080p; "UHD" alone must not promote it to 2160p.
        let current = r#"<h1 class='entry-title'>Download Film (2026) 2160p || 1080p</h1>
            <div class='entry-content'><p><strong>Film.1080p.UHD.x265 [6.73 GB]</strong></p>
            <p><a class='maxbutton-download-g-drive' title='Download From Google Drive'
                href='https://en.thenaukriadda.in/?sid=opaque'>Download (G-Drive)</a></p>
            </div>"#;
        let (_, current_releases) = post("/download-film-2026/", &base, current).unwrap();
        assert_eq!(current_releases.len(), 1);
        assert_eq!(current_releases[0].quality.as_deref(), Some("1080p"));
        assert_eq!(
            current_releases[0].size_bytes,
            Some((6.73 * 1_073_741_824.0) as u64)
        );

        // The extension also supports ordinary episode anchors beneath season headings.
        let plain = r#"<h1 class="entry-title">Download Lanterns S03</h1><div class="entry-content">
            <pre>Season 3</pre><p><a href="https://cloud.unblockedgames.world/?sid=plain">Episode 8</a>
            <a href="https://cloud.unblockedgames.world/?sid=next">Episode 9</a></p>
            </div>"#;
        let (details, releases) = post("/download-lanterns-s03/", &base, plain).unwrap();
        assert_eq!(details.seasons[0].number, 3);
        assert_eq!(
            releases.iter().map(|r| r.episode).collect::<Vec<_>>(),
            vec![Some(8), Some(9)]
        );
    }

    #[test]
    fn current_movie_posts_ignore_decoy_tag_links_and_keep_each_release() {
        let base = site::base_url("https://uhdmovies.example/").unwrap();
        let html = r#"<h1 class="entry-title">Download Inception (2010) Dual Audio 1080p 10Bit || 2160p 4k HEVC Bluray Esubs</h1>
            <div class="entry-content">
            <p>Download <strong>Inception (2010)</strong> in <strong><span><a href="https://gate.example/?sid=decoy1">4k</a></span>
               <a href="https://gate.example/?sid=decoy2"><span><span>2160p</span> HEVC</span></a></strong> and
               <a href="https://gate.example/?sid=decoy3"><strong>UHDMOVIES</strong></a>.</p>
            <h2>Download Inception 4k (2010) Hindi Dual Audio</h2>
            <p> Inception (2010) 2160p HDR 10bit Bluray HEVC [DD 5.1 Hindi + DD 7.1 English] x265-<span><strong>UHDMovie</strong></span>s [<strong><span>17.5GB</span></strong>]
            <p><span class='mb-center'><span class='mb-container'><a title="Download From Google Drive" class="maxbutton-1 maxbutton maxbutton-download-g-drive" href="https://gate.example/?sid=real1"><span class='mb-text'>Download (G-Drive)</span></a></span></span>
            <hr />
            <p> Inception (2010) 1080p 10bit Bluray [DD 5.1 Hindi + DD 5.1 English] x265-<span><strong>UHDMovies</strong></span> [<span><strong>7.07GB</strong></span>]
            <p><span class='mb-center'><span class='mb-container'><a title="Download From Google Drive" class="maxbutton-1 maxbutton maxbutton-download-g-drive" href="https://gate.example/?sid=real2"><span class='mb-text'>Download (G-Drive)</span></a></span></span>
            <p>Here you can download <a href="https://gate.example/?sid=decoy4"><strong>1080p x264 UHD</strong></a>,
               <a href="https://gate.example/?sid=decoy5">4k 2160p</a> and <a href="https://gate.example/?sid=decoy6">3D Movies</a>.</p>
            </div>"#;
        let (details, releases) = post("/download-inception-2010/", &base, html).unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(releases.len(), 2, "{releases:#?}");
        assert_eq!(releases[0].quality.as_deref(), Some("2160p"));
        assert_eq!(
            releases[0].size_bytes,
            Some((17.5 * 1_073_741_824.0) as u64)
        );
        assert_eq!(releases[1].quality.as_deref(), Some("1080p"));
        assert_eq!(
            releases[1].size_bytes,
            Some((7.07 * 1_073_741_824.0) as u64)
        );
        assert!(releases[1].filename.starts_with("Inception (2010) 1080p"));
        let urls: Vec<&str> = releases
            .iter()
            .flat_map(|r| r.mirrors.iter().map(|m| m.resolver_url.as_str()))
            .collect();
        assert_eq!(
            urls,
            vec![
                "https://gate.example/?sid=real1",
                "https://gate.example/?sid=real2"
            ]
        );
    }

    #[test]
    fn current_series_posts_keep_episode_qualities_and_skip_decoys_and_zips() {
        let base = site::base_url("https://uhdmovies.example/").unwrap();
        let html = r#"<h1 class="entry-title">Download Breaking Bad (Season 01-05) Dual Audio 1080p</h1>
            <div class="entry-content">
            <p>Download in <a href="https://gate.example/?sid=decoy1">1080p UHD</a> x264</p>
            <pre><b></b><span><b>SEASON 1</b></span></pre>
            <p><strong><span> Breaking Bad S01 1080p BluRay REMUX AVC [Hindi] ESubs</span></strong><br />
               <strong>[<span>11 </span><span>GB/<span>E</span></span>] [<span>73.07GB</span>/<span>Zip</span>]</strong></p>
            <p><a class="maxbutton-2 maxbutton maxbutton-gdrive-episode" href="https://gate.example/?sid=a1"><span class='mb-text'>Episode 1</span></a>
               <a class="maxbutton-2 maxbutton maxbutton-gdrive-episode" href="https://gate.example/?sid=a2"><span class='mb-text'>Episode 2</span></a>
               <a class="maxbutton-3 maxbutton maxbutton-gdrive-zip" href="https://gate.example/?sid=zip"><span class='mb-text'>Zip / Pack</span></a></p>
            <p><span><strong>Breaking Bad S01 720p BluRay x264 [Hindi] ESubs<br />[<span>3 GB/<span>E</span></span>]</strong></span></p>
            <p><a class="maxbutton-2 maxbutton maxbutton-gdrive-episode" href="https://gate.example/?sid=b1"><span class='mb-text'>Episode 1</span></a>
               <a class="maxbutton-2 maxbutton maxbutton-gdrive-episode" href="https://gate.example/?sid=b2"><span class='mb-text'>Episode 2</span></a></p>
            </div>"#;
        let (details, releases) = post("/download-breaking-bad/", &base, html).unwrap();
        assert_eq!(details.media_type, MediaType::Series);
        assert_eq!(details.seasons.len(), 1);
        assert_eq!(details.seasons[0].episodes.len(), 2);
        assert_eq!(releases.len(), 4, "{releases:#?}");
        let first: Vec<_> = releases.iter().filter(|r| r.episode == Some(1)).collect();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].quality.as_deref(), Some("1080p"));
        assert_eq!(first[0].size_bytes, Some(11 * 1_073_741_824));
        assert_eq!(first[1].quality.as_deref(), Some("720p"));
        assert!(releases.iter().all(|r| r.season == Some(1)));
        assert!(
            releases
                .iter()
                .flat_map(|r| &r.mirrors)
                .all(|m| { !m.resolver_url.contains("zip") && !m.resolver_url.contains("decoy") })
        );
    }

    #[test]
    fn season_heading_supplies_the_season_when_descriptions_omit_it() {
        let base = site::base_url("https://uhdmovies.example/").unwrap();
        let html = r#"<h1 class="entry-title">Download Show (Season 01-02) 1080p</h1>
            <div class="entry-content">
            <pre>SEASON 2</pre>
            <p><strong>Show 1080p WEB-DL [2 GB/E]</strong></p>
            <p><a class="maxbutton maxbutton-gdrive-episode" href="https://gate.example/?sid=x1"><span class='mb-text'>Episode 4</span></a></p>
            </div>"#;
        let (_, releases) = post("/download-show/", &base, html).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(
            (releases[0].season, releases[0].episode),
            (Some(2), Some(4))
        );
    }
}
