# Up Next overlay & episode autoplay

`scripts/mpv/up_next.lua` renders an "Up Next" card near the end of an episode and
starts the following episode when the current one finishes. It works for both
MovieBox-TUI network streams and plain local files.

## Why a sidecar file is needed

A player-side script normally finds the next episode by listing the folder of the
current file. MovieBox-TUI plays HTTP streams (direct CDN links, or `127.0.0.1`
links served by the loopback relay), so there is no directory to scan and a
folder-based script can only ever report *"Season Ended"*.

To solve this the application publishes the next episode to a small JSON sidecar
every time playback starts:

```jsonc
{
  "version": 1,
  "updated_at": 1764960000,
  "provider": "moviebox",
  "subject_id": "4242",
  "title": "Breaking Bad",
  "current": { "season": 1, "episode": 4, "url": "http://127.0.0.1:8099/stream" },
  "next": {
    "has_next": true,
    "season": 1,
    "episode": 5,
    "label": "S01E05",
    "title": "Gray Matter",
    "url": null,
    "subtitle": null,
    "headers": []
  },
  "request_file": "/storage/emulated/0/MovieBox-TUI/upnext_request.json"
}
```

Written by `src/player/upnext.rs` to:

| Platform | Path |
| :--- | :--- |
| Linux / Termux | `~/.local/share/moviebox-tui/playback/upnext.json` |
| macOS | `~/Library/Application Support/moviebox-tui/playback/upnext.json` |
| Windows | `%APPDATA%\moviebox-tui\playback\upnext.json` |
| Android (mirror) | `/storage/emulated/0/MovieBox-TUI/upnext.json` |

The Android mirror matters: `mpv-android` runs in its own app sandbox and cannot
read Termux's private data directory. Shared storage is the only location both
processes can reach, so the sidecar is written to both.

On desktop, mpv is additionally started with `moviebox-upnext_file=<path>` in
`--script-opts`, so the script reads the exact file instead of probing. Android
intent launches cannot carry script options, which is why the well-known paths
above exist.

## Installation

**Desktop (mpv / IINA)**

```bash
mkdir -p ~/.config/mpv/scripts
cp scripts/mpv/up_next.lua ~/.config/mpv/scripts/
```

**Android (mpv-android)**

Copy the script to the app's script folder:

```text
/storage/emulated/0/Android/data/is.xyz.mpv/files/scripts/up_next.lua
```

## Behaviour

- The card appears 2 minutes before the end (`TRIGGER_SECONDS`) with a live
  countdown.
- When the app published a next episode, the card shows the real show title and
  `SxxEyy` label; otherwise it shows **Season Ended**, which now only happens on
  an actual final episode.
- At end of file the script either:
  1. loads the next episode directly, when the sidecar carries a resolved
     `next.url` (seamless, no return to the terminal), or
  2. writes `upnext_request.json` and quits, which MovieBox-TUI picks up on its
     next tick and resolves + relaunches.
- mpv is launched with `--keep-open=no`, so the script temporarily forces
  `keep-open=yes` while a next episode exists, and releases it when there is none
  or when autoplay is cancelled.

### Keys

| Key | Action |
| :--- | :--- |
| `Enter` | Play the next episode immediately |
| `c` | Cancel autoplay for this episode |

### Configuration

Edit the constants at the top of the script:

| Option | Default | Meaning |
| :--- | :--- | :--- |
| `TRIGGER_SECONDS` | `120` | When the card appears, in seconds before the end |
| `AUTOPLAY` | `true` | Set `false` for an overlay-only, no-autoplay card |
| `AUTOPLAY_LEAD` | `0` | Start the next episode N seconds early (`0` = at EOF) |
| `SIDECAR_POLL_INTERVAL` | `5` | How often to re-read the sidecar while playing |
| `SIDECAR_MAX_AGE` | `21600` | Ignore sidecars older than this, in seconds |

Autoplay can also be disabled per launch with `--script-opts=moviebox-autoplay=no`.

## Staleness and safety

A sidecar is only used when it belongs to the stream being played:

1. when script options are present, `subject_id` and the current episode must match;
2. otherwise the recorded `current.url` must equal mpv's `path`;
3. failing both, only a sidecar written in the last 120 seconds is trusted.

Every launch rewrites the sidecar — including movies, final episodes, and Live TV,
which publish `has_next: false` — so a previous series can never leak into an
unrelated playback session.

## Local files

With no sidecar the script keeps the original behaviour: it lists the current
file's directory, sorts video files by name, and offers the next one. Autoplay
loads that file directly.
