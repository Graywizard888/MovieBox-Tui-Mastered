# Downloader

MovieBox-TUI includes a multi-segment HTTP chunked downloader supporting pause, resume, and authentication header forwarding.

## Overview

- **Storage Location**: Defaults to `~/Downloads/MovieBox-TUI/`. Configurable via `/settings` (General → Download Folder).
- **Multi-Segment Engine**: Files are partitioned into concurrent byte ranges using HTTP RFC 7233 `Range: bytes=X-Y` requests.
- **Single-Stream Fallback**: If an upstream server or CDN does not support range requests (returns HTTP `200 OK` instead of `206 Partial Content`), the engine falls back to single-stream downloading without failing.

## File Lifecycle & State Files

During download, files are saved with temporary extensions:

```text
destination.mp4.part       # Pre-allocated in-place byte buffer across all workers
destination.mp4.part.json  # Download state (ETag, Last-Modified, total size, per-segment byte offsets)
destination.mp4            # Final verified output (atomic rename on completion)
```

Upon completion, temporary sidecars are verified and renamed atomically to the target filename.

## Download Controls

- Press **`d`** on any stream in the Details screen to start downloading immediately.
- Press **`x`** or **`X`** during an active download to cancel or pause the transfer. Partial `.part` data is preserved on disk for resumption.
- Downloads run cooperatively in the background, allowing you to browse or search without interrupting transfers.

## Header Forwarding

Authenticated streams (such as MovieBox DASH manifests or 4KHDHub mirrors) automatically forward required headers (`User-Agent`, `Referer`, signed CloudFront cookies) to download workers, ensuring CDN transfers complete without `403 Forbidden` errors.

## DASH Streams (`yt-dlp`)

MovieBox DASH streams require `yt-dlp` and `ffmpeg` to download and mux adaptive video/audio representations:

- **Windows**: `winget install yt-dlp.yt-dlp Gyan.FFmpeg`
- **macOS**: `brew install yt-dlp ffmpeg`
- **Android (Termux)**: `pkg install yt-dlp ffmpeg`
- **Linux**: Install `yt-dlp` and `ffmpeg` via system package manager.
The downloader preflights both binaries before launching the transfer, failing fast with platform-specific installation commands if either is absent. Transferred fragments are pulled with `--concurrent-fragments 8` and `--http-chunk-size 95K` (keeping individual range sub-requests under Tengine's `limit_rate_after 96k` throttle boundary) alongside resilient retry bounds (`--fragment-retries 10`, `--retries 5`, `--socket-timeout 30`). Any subprocess errors during transfer capture and display the underlying `yt-dlp` error diagnostics directly in the failure notification.

All other providers (4KHDHub, UHDMovies, Moviesmod, ToonWorld4All, Dramachi, BDIX, DhakaFlix, CircleFTP, Stremio Addons, TV mode) download through the internal multi-segment HTTP engine after resolving any provider link pages to media URLs (4–12 concurrent workers writing in-place to the pre-allocated `.part` file). ToonWorld4All archive/movie mirrors that require an interactive ad shortener or return a landing page cannot be downloaded automatically.

## Season downloads from UHDMovies, Moviesmod and ToonWorld4All

These providers list several encodes per episode (often 2160p remuxes of 10–20 GB each), so
pressing `d` on a season asks which quality to use. The list shows each encode with its size;
the stream highlighted when you pressed `d` is preselected. Your choice is applied to every
episode in the queue, matching the same encode first, then the same resolution and codec, then
the nearest resolution. It asks again each time you start a season download, and `Esc`
cancels without queueing anything.

If a link cannot be resolved part-way through a season, the queue halts with a
"Season download halted" notice that says how many files finished.

Saved files use the container the host reports (`.mkv` for Matroska) instead of always `.mp4`.

