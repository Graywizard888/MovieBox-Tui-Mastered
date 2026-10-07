# Configuration

MovieBox-TUI stores its settings in `config.json` inside your platform configuration directory.

## Interactive Settings Hub (`/settings`)

Enter `/settings` in the search bar or click `[ ⚙ Settings ]` on the Home screen to configure options interactively:

- **General**: Toggle automatic update checks, select default media player (`mpv`, `VLC`, `IINA`, `Android`), and customize download directory.
- **Content Modes**: Toggle Streaming Mode, enable or disable specific streaming providers (MovieBox, 4KHDHub, UHDMovies, Moviesmod, ToonWorld4All, Dramachi, CircleFTP, DhakaFlix), and toggle Live TV Mode.
- **Appearance**: Select among 9 color themes (Mocha, Latte, Macchiato, Frappe, Nord, TokyoNight, Dracula, Gruvbox, RosePine) with live preview.
- **Maintenance**: Check for updates, re-test local BDIX network connectivity, clear disk cache, clear watch history, and open the GitHub repository.

## Configuration File Paths

- **Linux / macOS / Termux**: `~/.config/moviebox-tui/config.json`
- **Windows**: `%APPDATA%\MovieBox-Tui\config.json`

## Configuration Options (`config.json`)

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `auto_update` | `bool` | `true` | Check for application updates on launch |
| `default_player` | `string` | `"mpv"` | Default player (`"mpv"`, `"vlc"`, `"iina"`, `"android"`) |
| `download_dir` | `string?` | `None` | Custom download directory (defaults to `~/Downloads/MovieBox-TUI`) |
| `streaming_enabled` | `bool` | `true` | Enable on-demand streaming |
| `tv_enabled` | `bool` | `true` | Enable live TV mode |
| `addons_enabled` | `bool` | `false` | Enable Stremio Addons provider |
| `moviebox_enabled` | `bool` | `true` | Enable MovieBox streaming provider |
| `fourkhdhub_enabled` | `bool` | `true` | Enable 4KHDHub streaming provider |
| `uhdmovies_enabled` | `bool` | `true` | Enable UHDMovies streaming provider |
| `moviesmod_enabled` | `bool` | `true` | Enable Moviesmod streaming provider |
| `toonworld4all_enabled` | `bool` | `true` | Enable ToonWorld4All streaming provider |
| `dramachi_enabled` | `bool` | `true` | Enable Dramachi streaming provider |
| `bdix_circleftp_enabled` | `bool` | `false` | Enable CircleFTP (BDIX) provider |
| `bdix_dhakaflix_enabled` | `bool` | `false` | Enable DhakaFlix (BDIX) provider |
| `theme` | `string` | `"TokyoNight"` | Active color theme |
| `mpv_path` | `string?` | `None` | Custom executable path for `mpv` |
| `vlc_path` | `string?` | `None` | Custom executable path for `VLC` |
| `iina_path` | `string?` | `None` | Custom executable path for `IINA` (macOS) |

## Environment Variables

| Variable | Description |
| :--- | :--- |
| `MOVIEBOX_PLAYER` | Force media player (`"mpv"`, `"vlc"`, `"iina"`, `"android"`) |
| `MOVIEBOX_MPV_PATH` | Custom executable path for `mpv` |
| `MOVIEBOX_VLC_PATH` | Custom executable path for `VLC` |
| `MOVIEBOX_IINA_PATH` | Custom executable path for `IINA` (macOS) |
| `MOVIEBOX_CONFIG_DIR` | Custom directory for `config.json` |
| `MOVIEBOX_DATA_DIR` | Custom directory for watch history and favorites |
| `MOVIEBOX_CACHE_DIR` | Custom directory for disk cache |
| `MOVIEBOX_THEME` | Override active theme on launch (e.g. `"TokyoNight"`, `"Dracula"`) |
| `MOVIEBOX_LOG` | Logging level (`"error"`, `"warn"`, `"info"`, `"debug"`, `"trace"`) |
| `MOVIEBOX_UHDMOVIES_URL` | Override UHDMovies base URL (HTTPS) when its domain changes |
| `MOVIEBOX_UHDMOVIES_PREVIOUS_URL` | Trust an old UHDMovies permalink origin when cards still link to it |
| `MOVIEBOX_MOVIESMOD_URL` | Override Moviesmod base URL (HTTPS) when its domain changes |
| `MOVIEBOX_MOVIESMOD_PREVIOUS_URL` | Trust an old Moviesmod permalink origin when cards still link to it |
| `MOVIEBOX_TOONWORLD4ALL_URL` | Override ToonWorld4All WordPress base URL (HTTPS) |
| `MOVIEBOX_TOONWORLD4ALL_PREVIOUS_URL` | Trust an older WordPress permalink origin |
| `MOVIEBOX_TOONWORLD4ALL_ARCHIVE_URL` | Override ToonWorld4All movie/episode archive base URL (HTTPS) |
| `MOVIEBOX_TOONWORLD4ALL_PREVIOUS_ARCHIVE_URL` | Rebase archive links on an earlier archive origin |
| `MOVIEBOX_TOONWORLD4ALL_WORKER_URL` | Override ToonWorld4All redirect worker base URL (HTTPS) |
| `MOVIEBOX_TOONWORLD4ALL_PREVIOUS_WORKER_URL` | Rebase worker redirects on an earlier worker origin |
| `MOVIEBOX_TOONWORLD_COOKIE` | Cookies copied from a browser that already passed the ToonWorld4All archive's 24-hour ad gate (e.g. `user=...`). Sent only to the archive's own redirect pages; never logged. Can also be pasted in Settings (General, ToonWorld Cookie), which keeps it in a private file; the variable wins when both are set. |
| `MOVIEBOX_SEEK_PROXY_MAX_MB` | Largest file (MB, default `3000`) for which UHDMovies/Moviesmod/ToonWorld4All playback fakes seeking when the host cannot seek (see Players). `0` turns it off. |
| `MOVIEBOX_NO_IMAGE` | Set to `"1"` or `"true"` to disable terminal image previews |
| `MOVIEBOX_IMAGE_PROTOCOL` | Force image protocol (`"kitty"`, `"sixel"`, `"iterm2"`, or `"off"`) |
| `MOVIEBOX_CELL_SIZE` | Override font cell size for image scaling (e.g. `"10x20"`) |
| `NO_COLOR` | Force high-contrast monochrome mode |
