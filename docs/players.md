# Players

MovieBox-TUI delegates playback to external media players (`mpv`, `IINA`, `VLC`, or Android intent players).

## Detection Order

Players are detected in priority order and cached across runs:

- **macOS**: `IINA` → `mpv` → `VLC`
  - Searches `/Applications`, `~/Applications`, Homebrew, MacPorts, Nix, and native CLI tools.
- **Linux**: `mpv` → `VLC`
  - Searches `$PATH`, `~/.local/bin`, Flatpak exports (`io.mpv.Mpv`, `org.videolan.VLC`), and Snap.
- **Windows**: `mpv` → `VLC`
  - Searches `%LOCALAPPDATA%`, `Program Files`, WinGet packages, Scoop shims, Chocolatey, and Windows Registry `App Paths`.
- **Android / Termux**: `Android Intent` (headless CLI) or `mpv` → `VLC` → `Android Intent` (graphical X11/Wayland desktop)
  - In headless terminal environments without an active display server, dispatches directly to external Android media apps via `termux-open` or `am start`.
  - In graphical environments with an active display server (`$DISPLAY` or `$WAYLAND_DISPLAY`, such as Xfce in udroid/PRoot or Termux:X11), native desktop players (`mpv`/`VLC`) are prioritized so playback opens in a desktop window.

You can set a default player via `/settings` (Media Player), or override it with the `MOVIEBOX_PLAYER` environment variable.

## Player Invocations

### mpv
```bash
mpv --autofit=WxH --geometry=50%:50% --hwdec=auto-safe --stream-buffer-size=4M --idle=no --keep-open=no [OPTIONS] <url>
```
- **Tracking**: Injects `moviebox_tracker.lua` to track playback progress, total duration, and chosen stream filename, auto-restoring the exact stream mirror upon re-opening multi-stream titles (such as 4KHDHub releases).
- **Hardware Decoding & Direct Streams**: Enables `--hwdec=auto-safe` for hardware-accelerated 4K 10-bit HEVC HDR decoding and passes `--ytdl=no` on non-DASH direct streams (`.mkv`, `.mp4`, `.m3u8`) for immediate native `ffmpeg` demuxer startup.
- **Headers**: Custom stream headers (`User-Agent`, `Referer`) are passed via `--http-header-fields`.
- **DASH Manifests & Loopback Acceleration**: DASH `.mpd` streams route through the local `StreamRelay` proxy (`127.0.0.1:<port>`), which caches rewritten manifests in RAM, warm-prefetches initialization and opening fragments on sidecar startup, fetches `.m4s` segments via parallel `95 KB` HTTP `Range` sub-requests, and prefetches both the next 3 video segments (`N+1..N+3`) and matching audio segments (`chunk-stream3-N..N+1`).
- **Subtitles**: Remote subtitles are loaded directly via `--sub-file=<url>`.

### VLC
```bash
vlc --width=W --height=H --play-and-exit --network-caching=3000 --file-caching=3000 --http-reconnect --adaptive-logic=predictive [OPTIONS] <url>
```
- **Headers**: Mapped to `--http-user-agent` and `--http-referrer`.
- **DASH & CloudFront Streams**: Cookie-authenticated and DASH `.mpd` streams route through the local `StreamRelay` proxy sidecar (`127.0.0.1:<port>`) with HTTP/1.1 persistent connections (`Keep-Alive`), parallel `95 KB` `Range`-chunked `.m4s` segment fetching, lookahead segment prefetching (`N+1..N+3`), and resolution representation capping.
- **Seeking on UHDMovies / Moviesmod / ToonWorld4All**: Playback first prefers a mirror that answers byte-range requests (Driveseed/Driveleech *Resume Cloud* workers), the same idea 4KHDHub uses. Some files only have the *Instant* mirror (Google's download CDN), which ignores `Range`, so no player can seek. For those the stream is routed through the loopback proxy, which answers range requests itself by re-reading the file from the start and discarding the bytes before the target. Nothing is written to disk and memory use stays flat. Costs: a seek forward takes as long as downloading the skipped part, and when a player reads the end of an MKV at startup (its index) the proxy refuses that read if it would take more than 15 s (`MOVIEBOX_SEEK_PROXY_END_WAIT_SECS`), so playback starts at once and seeks scan forward instead. Files above `MOVIEBOX_SEEK_PROXY_MAX_MB` (default 3000 MB; a far seek in a big file costs download time) are played as-is, without seeking. 4K releases are almost always above that limit.
- **Subtitles**: Remote subtitles are pre-downloaded to temporary storage and passed via `--sub-file=<path>`.

### IINA (macOS)
```bash
iina-cli --keep-running --no-stdin --mpv-autofit=WxH [OPTIONS] <url>
```
- Forwards `--mpv-*` arguments to internal mpv core.
- Falls back to `open -a IINA <url>` if CLI tools are unlinked.

### Android / Termux
```bash
termux-open --chooser --content-type video/* <url>
```
- Opens the system app chooser, delegating playback to installed Android players (VLC, MX Player, Just Player, mpv-android).
- Subtitles are saved to shared storage (`~/storage/downloads/moviebox_subs`) and passed via intent extras.

## Watch History & Resume

- **Position Resumption**: In-progress items launch with `--start=<seconds>` (`mpv`/`IINA`) or `--start-time=<seconds>` (`VLC`).
- **Completion Detection**: Reaching ≥ 90% or stream EOF marks media as completed.
- **State Reconciliation**: `mpv` and `IINA` sync playback progress via background IPC state files, preventing wall-clock drift during pauses or seeks.

## Spawning & Process Safety

- Players and the `StreamRelay` sidecar launch in fully detached OS sessions (`libc::setsid()` with `SIGHUP` ignored on Unix/macOS/Linux/Android, and `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` on Windows) so closing the terminal window or quitting `moviebox-tui` never terminates active video playback.
- Player `stderr` is directed to a temporary log file rather than a parent pipe, preventing `SIGPIPE` / broken-pipe crashes when `moviebox-tui` exits while preserving full crash diagnostics when the TUI remains open.
- Clean exits (VLC exit code `1` with empty stderr, or Unix `SIGTERM`) are recognized as normal exits and update watch history without false crash popups.
