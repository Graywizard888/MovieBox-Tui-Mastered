# Installation

MovieBox-TUI is available across macOS, Linux, Windows, and Android (Termux).

---

## macOS and Linux

### Direct Terminal Script

No package manager required. Open Terminal and run:

```bash
curl -fsSL https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.sh | bash
```

---

## Windows

### Direct PowerShell Script

No package manager required. Open **PowerShell** and run:

```powershell
irm https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.ps1 | iex
```

> **SmartScreen prompt:** If Windows displays *"Windows protected your PC"*, click **More info** → **Run anyway**.
---

## Android (Termux)

Open Termux and run:

```bash
pkg update && pkg install -y curl tar termux-tools termux-am
curl -fsSL https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.sh | bash
termux-setup-storage
```

> [!IMPORTANT]
> Video playback on Android launches through your installed external media player (such as VLC, Just Player, or MX Player).
---

## Cargo (from Git)

Install directly from the repository using Cargo:

```bash
cargo install --git https://github.com/Graywizard888/MovieBox-Tui-Mastered --locked
```

---

## Compile from Source

Clone the repository and build the release binary:

```bash
git clone https://github.com/Graywizard888/MovieBox-Tui-Mastered.git
cd MovieBox-Tui-Mastered
cargo build --release --locked
```

The compiled binary will be located at `target/release/moviebox-tui`.

---

## Verify Release Integrity

All release assets include cryptographically signed SHA-256 checksums and GitHub provenance attestations:

```bash
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify <archive-file> -R Graywizard888/MovieBox-Tui-Mastered
```
---

## Uninstallation

### Automated Installer (macOS, Linux, Windows, Android)

Simply re-run your original install command (`curl ... | bash` or `irm ... | iex`). When MovieBox-TUI is already installed, the installer automatically detects it and displays an interactive menu:

```text
MovieBox-TUI is already installed.
What would you like to do?
  1) Reinstall / Update to latest version
  2) Uninstall
  3) Cancel
```

Enter `2` to completely remove MovieBox-TUI from your system.

### Cargo

```bash
cargo uninstall moviebox-tui
```
