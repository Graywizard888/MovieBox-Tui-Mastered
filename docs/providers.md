# Streaming Providers

MovieBox-TUI searches and streams media across multiple independent providers.

## Available Providers

| Provider | Description |
| :--- | :--- |
| **MovieBox** | Primary streaming catalog with multi-language audio, seasons, and subtitles. |
| **4KHDHub** | High-bitrate 4K UHD and 1080p releases with fast CDN mirrors. |
| **UHDMovies** | WordPress movies and series with per-quality G-Drive releases. |
| **Moviesmod** | Movies and web series with per-episode Driveleech/Driveseed links. |
| **ToonWorld4All** | Animated movies and seasons from its WordPress catalog; archive episode qualities and mirrors. |
| **Dramachi** | Asian dramas and series catalog. |
| **CircleFTP** | High-speed local BDIX mirror (Bangladesh ISPs). |
| **DhakaFlix** | Local BDIX media indexer (Bangladesh ISPs). |
| **Addons** | Community Stremio HTTP addons (Cinemeta catalog and streams). |

## Switching Providers

- **Cycle Provider**: Press **`Ctrl+P`** on the home screen to switch to the next provider.
- **Provider Menu**: Click the provider badge in the search bar (or press `Enter` on it) to open the provider selection menu.
- **Enable / Disable Providers**: Open `/settings` → **Content Modes** → **Streaming Sources** to toggle providers on or off.

UHDMovies and Moviesmod releases are link pages, not direct video URLs. The app follows the
landing/download pages when a release is selected, checks that the resulting URL is media,
and only then passes it to the player or downloader. Some currently published links go through
`en.thenaukriadda.in` and show an **“I'm Not a Robot”** browser-verification page. The app
reports this gate rather than treating HTML as video; choose another mirror if one is available.
Dead links and site changes can also require trying a different release. No Cloudstream
installation is needed. The parsers
follow the [UHDMovies extension](https://github.com/phisher98/cloudstream-extensions-phisher/blob/builds/UHDmoviesProvider.cs3)
and [Moviesmod extension](https://github.com/SaurabhKaperwan/CSX/tree/master/Moviesmod).

These websites may change domains. UHDMovies and Moviesmod check their extensions' domain
lists on first use, again every 15 minutes, and (at most once per minute) after a network
error, HTTP 403/410, HTTP 404 on a first page/post, or HTTP 5xx. They retry a failed
request at most once when a different candidate origin is available. A working redirect
to another domain is adopted after a recognizable search/post response. If the list is
unavailable, the last known domain is retained; a lagging list cannot undo a confirmed
redirect to its old domain. Initial
fallbacks are `https://uhdmovies.my/` and `https://moviesmod.ai.in/`. A changed domain
with neither a working redirect nor an updated list still needs a manual
HTTPS override:

```sh
MOVIEBOX_UHDMOVIES_URL=https://new-uhdmovies.example/ \
MOVIEBOX_MOVIESMOD_URL=https://new-moviesmod.example/ moviebox-tui
```

Set only the variables you need. If the new site's search cards still contain absolute
permalinks from an older domain *other than the built-in fallback*, set
`MOVIEBOX_UHDMOVIES_PREVIOUS_URL` or `MOVIEBOX_MOVIESMOD_PREVIOUS_URL` to that old HTTPS origin.
These links are accepted only as paths on the configured/current site; unknown external
permalinks remain blocked. Restart after changing environment variables.

ToonWorld4All searches the WordPress posts (with an HTML fallback) and loads movie **and**
series-episode mirrors by labelled quality from the separate archive when selected. Older
posts can contain movie mirrors directly. Archive redirects display a destination file host;
some mirrors can instead lead to an interactive ad shortener. Playback and downloads resolve
links and check the response is media first. **Ad-gated, expired, or unrecognized mirrors
cannot be played or downloaded automatically**; choose a different mirror/quality. The
archive may list quality tabs whose files are not exposed in the page HTML; only actual
file links are offered. No shortener bypass or browser automation is bundled.

ToonWorld4All has **no authoritative auto-discovery list**. Working redirects from the old
WordPress/archive site are learned during the session; archive links on the current site's
`archive.<site-domain>` subdomain can also be recognized. If the old site is offline or
its archive/worker moves independently, set the appropriate HTTPS overrides:

```sh
MOVIEBOX_TOONWORLD4ALL_URL=https://new-toonworld.example/ \
MOVIEBOX_TOONWORLD4ALL_ARCHIVE_URL=https://new-archive.example/ \
MOVIEBOX_TOONWORLD4ALL_WORKER_URL=https://new-worker.example/ moviebox-tui
```

Set only the variables you need, then restart. Recognized post permalinks, movie/episode
archive paths, and redirect wrappers still pointing to the original `toonworld4all.me` or
`backend.tw4all.workers.dev` are rebased to the configured replacements. For an
*additional* earlier hostname embedded in posts, set `MOVIEBOX_TOONWORLD4ALL_PREVIOUS_URL`,
`MOVIEBOX_TOONWORLD4ALL_PREVIOUS_ARCHIVE_URL`, or
`MOVIEBOX_TOONWORLD4ALL_PREVIOUS_WORKER_URL` to its old HTTPS origin. Rebased paths and
worker tokens may still have expired; arbitrary external links are not assumed to be
archives. The three providers intentionally do **not** reuse their old unscoped on-disk
search/details/stream caches after a domain rotation; this makes revisiting a page fetch it
again (and may increase network requests). In-memory previews or selected streams may still
need a manual refresh with **`r`** after a mid-session move. Domain changes cannot repair
changed page layouts, ad verification, or dead third-party file hosts.

**Live spot-check (29 September 2026):** UHDMovies search and movie/series post links were
visible for *The Runner* and *Lovely Runner*; Moviesmod search, movie-quality pages, and
series episode links were visible for *Mile End Kicks* and *Avatar: The Last Airbender*;
ToonWorld4All's WordPress posts and the active file tabs in its *Toy Story 5* movie and
*Avatar* episode archives were visible. The sampled UHDMovies/Moviesmod `sid` destination
showed browser verification, and sampled ToonWorld4All file-host URLs returned 404. These
checks verify published catalog/link shapes, **not** successful playback or a complete
media download. Offline mock tests cover the corresponding application paths.

## BDIX Network Detection

If you are connected through a Bangladeshi ISP supporting BDIX:
- MovieBox-TUI automatically tests local BDIX mirrors on startup and enables them if reachable.
- You can manually re-test your connection anytime via `/settings` → **Maintenance** → **Re-check BDIX Network**.

## Provider Architecture (For Developers)

All providers implement a shared Rust trait in `src/providers/`:

```rust
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderKind;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn search(&self, query: &str, page: usize) -> Result<Vec<CatalogItem>, ProviderError>;
    async fn details(&self, id: &str) -> Result<MediaDetails, ProviderError>;
}
```

New providers implement `Provider` (and optionally `ReleaseProvider` for multi-mirror streams), then register in `src/service.rs`. Responses are automatically cached as MessagePack envelopes.
