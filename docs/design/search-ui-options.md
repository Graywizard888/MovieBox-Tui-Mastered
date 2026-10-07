# Search results UI — change options

Design proposal. Covers what happens after a title is typed into the search bar
and how the result set should be presented.

All mockups are drawn at a real terminal width so the density claims are
verifiable. The frame border is not part of the UI; it marks the terminal edge.

---

## 1. What the UI does today

Relevant code: `src/tui/screens/home.rs` (`draw`, `search_results_layout`,
`item_slot_rects`), `src/tui/state.rs` (`result_metrics`, `result_columns_for`).

- `result_columns_for(width)` returns **1 column below 110 columns**, 2 below
  160, 3 below 220. Nearly every real terminal — and every phone — runs the
  single-column path.
- `row_height = poster_rows + 1`, and `poster_rows = ceil(96px / cell_height)`,
  so a typical desktop terminal gets `poster_rows ≈ 6` → **7 rows per result**.
- Each slot is a poster on the left and three text rows on the right:
  title + resolution badge, then `★ rating  year  type`, then the provider badge.

At 80×24 that yields **two and a half visible results**:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Search: dune                                               [MovieBox ▾]     │
│                                                                              │
│  ▐▀▀▀▀▀▀▀▌  Dune: Part Two                                      [1080p]      │
│  ▐       ▌  ★ 8.5  2024  Movie                                               │
│  ▐ poster▌  [MovieBox]                                                       │
│  ▐       ▌                                                                   │
│  ▐▄▄▄▄▄▄▄▌                                                                   │
│                                                                              │
│  ▐▀▀▀▀▀▀▀▌  Dune                                                [2160p]      │
│  ▐       ▌  ★ 8.0  2021  Movie                                             ▓ │
│  ▐ poster▌  [MovieBox]                                                     ▓ │
│  ▐       ▌                                                                 ░ │
│  ▐▄▄▄▄▄▄▄▌                                                                 ░ │
│                                                                              │
│  ▐▀▀▀▀▀▀▀▌  Dune: Prophecy                                      [1080p]      │
│  ▐       ▌  ★ 7.1  2024  Series                                              │
│  ▐ poster▌  [MovieBox]                                                       │
│                                                                              │
│  Found 18 results on MovieBox.                                               │
│  [^S] Stream  ·  [^T] TV       [?] Help  [q] Quit                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

Problems this creates:

1. **Density.** 18 results need 7 screens of scrolling.
2. **Dead space.** The title row reserves the full width but puts the badge hard
   right, leaving a 40-column gap mid-row.
3. **No comparison.** Picking between three versions of the same film means
   scrolling back and forth; nothing is side by side.
4. **Ratings arrive late.** `★ 8.5` only renders when the title is already in
   `preview_cache`, so most rows show year and type alone on first paint.
5. **Multi-column never triggers** on the widths people actually use.

---

## Option A — Density pass (keep the card layout)

Smallest possible change: compress the existing card instead of redesigning it.

- `poster_rows` for search slots capped at 4 → `row_height = 5`.
- Merge the meta row and the provider row into one line.
- Move the quality badge next to the title instead of flush right.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Search: dune                                               [MovieBox ▾]     │
│                                                                              │
│  ▐▀▀▀▀▌  Dune: Part Two  [1080p]                                             │
│  ▐post▌  ★ 8.5 · 2024 · Movie · MovieBox                                     │
│  ▐▄▄▄▄▌                                                                      │
│  ▐▀▀▀▀▌  Dune  [2160p]                                                       │
│  ▐post▌  ★ 8.0 · 2021 · Movie · MovieBox                                   ▓ │
│  ▐▄▄▄▄▌                                                                    ▓ │
│  ▐▀▀▀▀▌  Dune: Prophecy  [1080p]                                           ░ │
│  ▐post▌  ★ 7.1 · 2024 · Series · MovieBox                                  ░ │
│  ▐▄▄▄▄▌                                                                    ░ │
│  ▐▀▀▀▀▌  Dune: Part One — Extended                                         ░ │
│  ▐post▌  ★ 8.0 · 2021 · Movie · 4KHD                                       ░ │
│  ▐▄▄▄▄▌                                                                      │
│                                                                              │
│  Found 18 results on MovieBox.                                               │
│  [^S] Stream  ·  [^T] TV       [?] Help  [q] Quit                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Density:** 2.5 → 4 results. **Posters:** kept, smaller.
**Code:** metrics in `result_metrics`, row composition in `home.rs` only.
**Effort:** small. **Risk:** low — no new state, no new keys.

---

## Option B — Compact table (no posters)

A scan-and-compare view. One line per result, aligned columns, sortable.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Search: dune                                               [MovieBox ▾]     │
│                                                                              │
│     TITLE                                   YEAR  TYPE     ★     QUAL  SRC   │
│  ▸  Dune: Part Two                          2024  Movie    8.5   1080p  MBox │
│     Dune                                    2021  Movie    8.0   2160p  MBox │
│     Dune: Prophecy                          2024  Series   7.1   1080p  MBox │
│     Dune: Part One — Extended Edition       2021  Movie    8.0   2160p  4KHD │
│     Dune (1984)                             1984  Movie    6.3   1080p  MBox │
│     Dune: The Sisterhood                    2023  Series   —     720p   UHD  │
│     Jodorowsky's Dune                       2013  Movie    8.1   1080p  MBox │
│     Dune Drifter                            2020  Movie    3.6   720p   MBox │
│     Children of Dune                        2003  Series   7.5   1080p  MBox │
│     Frank Herbert's Dune                    2000  Series   7.0   720p   UHD  │
│     Dune: Part Two (Hindi)                  2024  Movie    8.5   1080p  MBox │
│                                                                              │
│  18 results · sort: relevance  [s] sort  [v] view  [Enter] open              │
│  [^S] Stream  ·  [^T] TV       [?] Help  [q] Quit                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Density:** 2.5 → 11 results. Sort by relevance / year / rating via `s`.
**Trade-off:** posters disappear, which is a real loss on Kitty/Sixel terminals.
**Code:** new render path + a `view_mode` in `AppState` and config.
**Effort:** medium. **Risk:** low, it is additive — `v` toggles back to cards.

---

## Option C — Split view: list + preview pane  ★ recommended

List on the left, details for the highlighted row on the right. This is the
standard TUI answer (ranger, lf, aerc) and it fixes density *and* the
"can't compare" problem without giving up poster art.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Search: dune                                               [MovieBox ▾]     │
│                                                                              │
│  ▸ Dune: Part Two          2024 ★8.5 │  ▐▀▀▀▀▀▀▀▀▀▀▀▌                        │
│    Dune                    2021 ★8.0 │  ▐           ▌  Dune: Part Two        │
│    Dune: Prophecy       S1 2024 ★7.1 │  ▐  poster   ▌  2024 · Movie · 2h 46m │
│    Dune: Part One — Ext.   2021 ★8.0 │  ▐           ▌  ★ 8.5  [1080p] [HDR]  │
│    Dune (1984)             1984 ★6.3 │  ▐           ▌  [MovieBox]            │
│    Dune: The Sisterhood S1 2023 ★ —  │  ▐▄▄▄▄▄▄▄▄▄▄▄▌                        │
│    Jodorowsky's Dune       2013 ★8.1 │                                       │
│    Dune Drifter            2020 ★3.6 │  Paul Atreides unites with the        │
│    Children of Dune     S3 2003 ★7.5 │  Fremen to wage war against House     │
│    Frank Herbert's Dune S1 2000 ★7.0 │  Harkonnen, while facing a future     │
│    Dune: Part Two (Hindi)  2024 ★8.5 │  only he can foresee.                 │
│    Spice Wars              2022 ★5.9 │                                       │
│                                      │  Villeneuve · Chalamet, Zendaya       │
│  18 results                          │  Sci-Fi, Adventure                    │
│  [^S] Stream  ·  [^T] TV       [?] Help  [q] Quit                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Density:** 2.5 → 12 results, while *more* metadata is visible, not less.
- The preview pane is fed by `state.search_preview` / `preview_cache`, which the
  app already fetches for the highlighted row — no new network calls.
- One poster is decoded per selection instead of one per visible card, which
  also cuts image work.

Below ~90 columns the pane does not fit, so it collapses to the Option A list
and the preview moves behind `i` (the existing synopsis modal). Phone portrait,
55 columns:

```
┌─────────────────────────────────────────────────────┐
│  Search: dune                      [MovieBox ▾]     │
│                                                     │
│  ▸ Dune: Part Two              2024  ★8.5  1080p    │
│    Dune                        2021  ★8.0  2160p    │
│    Dune: Prophecy           S1 2024  ★7.1  1080p    │
│    Dune: Part One — Ext.       2021  ★8.0  2160p    │
│    Dune (1984)                 1984  ★6.3  1080p    │
│    Dune: The Sisterhood     S1 2023  ★ —   720p     │
│    Jodorowsky's Dune           2013  ★8.1  1080p    │
│    Dune Drifter                2020  ★3.6  720p     │
│                                                     │
│  18 results · [i] details                           │
│  [S] Stream · [T] TV      [?] [q]                   │
└─────────────────────────────────────────────────────┘
```

**Code:** new split layout in `home.rs`, a `preview_pane` flag, reuse of
`render_poster` and the overview text already in `MediaDetails`.
**Effort:** medium-large. **Risk:** medium — the layout branches by width, and
mouse hit-testing in `mouse.rs` needs the new rects.

---

## Option D — Grouped by source

Only worth doing together with federated search. Results cluster under the
provider that returned them, so duplicates across sources read as alternatives
rather than noise.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Search: dune                                              [All Sources ▾]   │
│                                                                              │
│  MovieBox ─────────────────────────────────────────────────────── 6 results  │
│  ▸ Dune: Part Two                      2024  Movie   ★8.5   1080p            │
│    Dune                                2021  Movie   ★8.0   2160p            │
│    Dune: Prophecy                   S1 2024  Series  ★7.1   1080p            │
│                                                        … 3 more  [→]         │
│                                                                              │
│  4KHDHub ──────────────────────────────────────────────────────── 4 results  │
│    Dune: Part Two — IMAX                2024  Movie   ★8.5   2160p           │
│    Dune: Part One — Extended            2021  Movie   ★8.0   2160p           │
│                                                        … 2 more  [→]         │
│                                                                              │
│  UHDMovies ────────────────────────────────────────────────────── 2 results  │
│    Dune Collection (2021-2024)          2024  Movie   ★ —    2160p           │
│                                                                              │
│  12 results from 3 of 8 sources · 2 still searching ⠙                        │
│  [^S] Stream  ·  [^T] TV       [?] Help  [q] Quit                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

Each group collapses to 3 rows with `… N more`, so one slow provider cannot push
everything else off screen, and groups can stream in as each provider answers.

**Effort:** medium, and it depends on federated search landing first.

---

## Option E — Best match + alternates

One hero card for the top hit, everything else compact underneath. Good when the
query is unambiguous ("dune" → almost certainly *Dune: Part Two*).

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Search: dune                                               [MovieBox ▾]     │
│                                                                              │
│  ▐▀▀▀▀▀▀▀▀▀▌   Dune: Part Two                              ★ 8.5   [1080p]   │
│  ▐         ▌   2024 · Movie · 2h 46m · Sci-Fi                                │
│  ▐ poster  ▌   Paul Atreides unites with the Fremen to wage war against      │
│  ▐         ▌   House Harkonnen.                                              │
│  ▐▄▄▄▄▄▄▄▄▄▌   [MovieBox]              [Enter] Play   [d] Download   [f] ★   │
│                                                                              │
│  Other matches ───────────────────────────────────────────────────────────   │
│    Dune                                 2021  Movie   ★8.0   2160p  MovieBox │
│    Dune: Prophecy                    S1 2024  Series  ★7.1   1080p  MovieBox │
│    Dune: Part One — Extended Edition    2021  Movie   ★8.0   2160p  4KHD     │
│    Dune (1984)                          1984  Movie   ★6.3   1080p  MovieBox │
│    Jodorowsky's Dune                    2013  Movie   ★8.1   1080p  MovieBox │
│    Children of Dune                  S3 2003  Series  ★7.5   1080p  MovieBox │
│                                                                              │
│  18 results                                                                  │
│  [^S] Stream  ·  [^T] TV       [?] Help  [q] Quit                            │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Risk:** when the ranking is wrong, the hero card is actively misleading. The
current sort is lexical (exact match → prefix → type → year), so I would not
promote a hero card until ranking also weighs rating and popularity.

---

## Cross-cutting improvements (apply to whichever option wins)

These are independent of the layout choice and mostly cheap:

1. **Lower the multi-column threshold.** `result_columns_for` jumps to 2 columns
   at 110; at ~92 two columns already fit comfortably in the card layout.
2. **Highlight the matched substring** of the query inside each title, so it is
   obvious why a row matched.
3. **Kill the title-row gap.** Put the quality badge immediately after the
   title; reserve the right edge for the source badge only.
4. **Skeleton rows while loading** instead of a centred spinner — the list
   frame appears instantly and fills in, which reads as much faster.
5. **Prefetch ratings for visible rows.** Ratings currently populate only for
   the highlighted item, so the `★` column looks broken on first paint. Fetch
   previews for the visible window, not just the selection.
6. **Keep the selection stable** across pagination (already fixed for append,
   worth asserting in a test for the grouped/streamed cases).
7. **Richer result counter**: `18 results · 0.9s · MovieBox` instead of
   `Found 18 results on MovieBox.`
8. **Empty state with intent**: offer `[Ctrl+P] try another source`,
   `[r] retry`, and the two nearest suggestions rather than a bare message.

---

## Recommendation

**Option C (split view) with the Option A density pass as its narrow-width
fallback**, then Option D once federated search lands.

Rationale: C is the only option that increases information density *and*
keeps poster art, and it reuses preview data the app already fetches. A is
effectively free and is required anyway as the sub-90-column fallback. D layers
cleanly on top of C's list pane later.

Suggested sequencing:

| Step | Scope | Effort |
| :--- | :--- | :--- |
| 1 | Option A density + cross-cutting items 1, 3, 7 | small |
| 2 | Option C split pane above 90 columns, A below | medium |
| 3 | Cross-cutting 2, 4, 5 (match highlight, skeletons, visible-window ratings) | small-medium |
| 4 | Option D grouping, after federated search | medium |

A `view_mode` setting (`cards` / `list` / `split`) in `Config` plus a `v` key
makes B reachable for anyone who prefers raw density, without another fork in
the render path beyond the one C already introduces.
