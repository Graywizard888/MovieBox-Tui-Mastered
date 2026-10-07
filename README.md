<div align="center">

# MovieBox-TUI

**Terminal interface to find, download, and stream movies, TV shows, and live TV using local media players.**

A fork of [mesamirh/MovieBox-Tui](https://github.com/mesamirh/MovieBox-Tui).

[ English ](README.md) • [ বাংলা ](README_BN.md) • [ हिन्दी ](README_HI.md) • [ Español ](README_ES.md)

[![CI](https://img.shields.io/github/actions/workflow/status/Graywizard888/MovieBox-Tui-Mastered/ci.yml?branch=main&label=CI&logo=github&style=flat)](https://github.com/Graywizard888/MovieBox-Tui-Mastered/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Graywizard888/MovieBox-Tui-Mastered?label=release&logo=github&style=flat)](https://github.com/Graywizard888/MovieBox-Tui-Mastered/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg?style=flat)](#license)
[![Telegram](https://img.shields.io/badge/Telegram-Channel-2CA5E0?style=flat&logo=telegram&logoColor=white)](https://t.me/getfromme)
[![Support](https://img.shields.io/badge/Support-Crypto-F7931A?style=flat&logo=bitcoin&logoColor=white)](#optional-support)
</div>

[moviebox-tui-walkthrough.webm](https://github.com/user-attachments/assets/7554a7e5-6ff5-49ec-9d87-f821ea99950e)

## Features

- **Streaming:** Movies, TV series, anime, Asian dramas, and community Stremio addons across multiple native providers.
- **Live TV:** M3U playlist import with channel categories, EPG support, and search.
- **Resolution Picker:** Direct stream quality selection (`4K`, `1080p`, `720p`, `480p`, `Auto`) before playback.
- **Hardware Players:** Seamless launch in `mpv`, `VLC`, or `IINA` with custom auth header and cookie forwarding.
- **Batch Downloader:** Multi-segment concurrent downloader with HTTP range pause and resume for episodes and full seasons.
- **Subtitle Picker:** Multi-language subtitle tracks extracted and selectable via an interactive picker before playback or download.
- **Terminal UI:** Vim navigation, mouse interaction, slash command palette (`/help`, `/settings`), and 9 built-in themes.
- **Cover Art:** Native Kitty, Sixel, and iTerm2 poster rendering with automatic text fallback.
- **Resume & Library:** Home deck with continue-watching timestamps, watch history, and favorites stored strictly on local disk. Zero telemetry.

## Prerequisites

- **Media Player:** `mpv`, `VLC`, or `IINA` (macOS) / any external video player (Android).
- **Posters:** Terminal with Sixel, Kitty, or iTerm2 support (Ghostty, Kitty, WezTerm, iTerm2, foot, Windows Terminal v1.22+).
- **DASH Downloads:** `yt-dlp` and `ffmpeg` (required only for MovieBox DASH downloads).

## Installation

### macOS & Linux

Direct install via Terminal (no package manager needed):
```bash
curl -fsSL https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.sh | bash
```

### Windows

Direct install via PowerShell (no package manager needed):
```powershell
irm https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.ps1 | iex
```

> **SmartScreen prompt:** If Windows displays *"Windows protected your PC"*, click **More info** → **Run anyway**.

### Android (Termux)
```bash
pkg update && pkg install -y curl tar termux-tools termux-am
curl -fsSL https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.sh | bash
termux-setup-storage
```

<details>
<summary><b>Cargo & Source Build</b></summary>

```bash
cargo install --git https://github.com/Graywizard888/MovieBox-Tui-Mastered --locked
```

From source:
```bash
git clone https://github.com/Graywizard888/MovieBox-Tui-Mastered.git
cd MovieBox-Tui-Mastered
cargo build --release --locked
```

</details>

<details>
<summary><b>Verify Release Integrity</b></summary>

```bash
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify <archive-file> -R Graywizard888/MovieBox-Tui-Mastered
```

</details>

<details>
<summary><b>Uninstall</b></summary>

Re-run install command (`curl ... | bash` or `irm ... | iex`) and select `2) Uninstall`.

If you installed with Cargo:
```bash
cargo uninstall moviebox-tui
```

</details>

## Quick Start

```bash
moviebox-tui
```

- Type to search, press `Enter` to play.
- Press `?` for shortcuts, or type `/settings` for preferences.

## Documentation

Full documentation in [`docs/`](docs/):

| Guide | Description |
| :--- | :--- |
| [Keyboard & Controls](docs/controls.md) | Keybindings, vim navigation, and shortcuts |
| [Configuration](docs/config.md) | Settings, themes, and environment variables |
| [Content Providers](docs/providers.md) | Native scrapers (MovieBox, 4KHDHub, UHDMovies, Moviesmod, ToonWorld4All, Dramachi, BDIX) |
| [Stremio Addons](docs/addons-mode.md) | Community addon configuration and streaming |
| [Hardware Players](docs/players.md) | Player detection, launch options, and flags |
| [Live TV & IPTV](docs/tv-mode.md) | M3U playlist import and channel streaming |
| [Batch Downloads](docs/downloads.md) | HTTP range engine, pause, and resume |

## Contributing

Review [CONTRIBUTING.md](CONTRIBUTING.md) before submitting pull requests. Report bugs via [GitHub Issues](https://github.com/Graywizard888/MovieBox-Tui-Mastered/issues).

<details>
<summary><b>Support</b></summary>
<div id="optional-support" tabindex="-1"></div>

| Asset | Address |
| :--- | :--- |
| **USDT (TRC20)** | `TL4yW73qmbKZpBWwbEFgjBpwVkPDFTkJgV` |
| **Bitcoin (BTC)** | `3MEAtqtRWrQBhnaMi3Zuf5nt2efNUS2LUQ` |
| **Ethereum (EVM)** | `0x7ea20d5fa29d87f33195f5a3b211ff94038d794c` |
| **Solana (SOL)** | `6ctm5WFv73MNywoCKAz3xK72yizSspHa72rFNygooU6` |

</details>

## Privacy

Zero telemetry, analytics, or user tracking. History, favorites, and config remain local.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

## Disclaimer

MovieBox-TUI does not host or store media. It plays publicly available streams. Users must comply with local laws.
