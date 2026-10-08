<div align="center">

# MovieBox-TUI

**Interfaz de terminal para buscar, descargar y transmitir películas, series y TV en vivo mediante reproductores locales.**

[ English ](README.md) • [ বাংলা ](README_BN.md) • [ हिन्दी ](README_HI.md) • [ Español ](README_ES.md)

[![CI](https://img.shields.io/github/actions/workflow/status/Graywizard888/MovieBox-Tui-Mastered/ci.yml?branch=main&label=CI&logo=github&style=flat)](https://github.com/Graywizard888/MovieBox-Tui-Mastered/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Graywizard888/MovieBox-Tui-Mastered?label=release&logo=github&style=flat)](https://github.com/Graywizard888/MovieBox-Tui-Mastered/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg?style=flat)](#licencia)
[![Telegram](https://img.shields.io/badge/Telegram-Channel-2CA5E0?style=flat&logo=telegram&logoColor=white)](https://t.me/getfromme)
[![Support](https://img.shields.io/badge/Support-Crypto-F7931A?style=flat&logo=bitcoin&logoColor=white)](#apoyo-opcional)
</div>

[moviebox-tui-walkthrough.webm](https://github.com/user-attachments/assets/7554a7e5-6ff5-49ec-9d87-f821ea99950e)

## Características

- **Streaming:** Películas, series, anime, dramas asiáticos y complementos comunitarios de Stremio en múltiples proveedores nativos.
- **TV en vivo:** Importación de listas M3U con categorías de canales, soporte EPG y búsqueda.
- **Selector de resolución:** Selección directa de calidad de stream (`4K`, `1080p`, `720p`, `480p`, `Auto`) antes de reproducir.
- **Reproductores por hardware:** Inicio directo en `mpv`, `VLC` o `IINA` con reenvío de cabeceras de autenticación y cookies.
- **Descargador por lotes:** Descargador concurrente multisegmento con pausa y reanudación por rangos HTTP para episodios y temporadas completas.
- **Selector de subtítulos:** Pistas de subtítulos en múltiples idiomas extraídas y seleccionables mediante un selector interactivo antes de reproducir o descargar.
- **Interfaz de terminal:** Navegación Vim, soporte de ratón, paleta de comandos (`/help`, `/settings`) y 9 temas integrados.
- **Carátulas:** Renderizado nativo de pósters con Kitty, Sixel e iTerm2 con alternativa automática en texto.
- **Reanudación y biblioteca:** Marcas de tiempo de continuar viendo, historial y favoritos almacenados localmente en disco. Cero telemetría.

## Requisitos previos

- **Reproductor multimedia:** `mpv`, `VLC` o `IINA` (macOS) / cualquier reproductor de video externo (Android).
- **Pósters:** Terminal con soporte Sixel, Kitty o iTerm2 (Ghostty, Kitty, WezTerm, iTerm2, foot, Windows Terminal v1.22+).
- **Descargas DASH:** `yt-dlp` y `ffmpeg` (requeridos solo para descargas DASH de MovieBox).

## Instalación

### macOS y Linux

Instalación directa mediante Terminal (macOS y Linux, sin necesidad de gestor de paquetes):
```bash
curl -fsSL https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.sh | bash
```

### Windows

Instalación directa mediante PowerShell (sin necesidad de gestor de paquetes):
```powershell
irm https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.ps1 | iex
```

> **Aviso de SmartScreen:** Si Windows muestra *"Windows protegió su PC"*, haz clic en **Más información** → **Ejecutar de todas formas**.

### Android (Termux)

Abre Termux y ejecuta:
```bash
pkg update && pkg install -y curl tar termux-tools termux-am
curl -fsSL https://raw.githubusercontent.com/Graywizard888/MovieBox-Tui-Mastered/main/install.sh | bash
termux-setup-storage
```
> [!IMPORTANT]
> La reproducción de video en Android se abre mediante tu reproductor externo instalado (como VLC, Just Player, o MX Player).

<details>
<summary><b>Instalación con Cargo o compilación desde el código fuente</b></summary>

Desde Git:
```bash
cargo install --git https://github.com/Graywizard888/MovieBox-Tui-Mastered --locked
```

Compilar desde el código fuente:
```bash
git clone https://github.com/Graywizard888/MovieBox-Tui-Mastered.git
cd MovieBox-Tui-Mastered
cargo build --release --locked
```

</details>

<details>
<summary><b>Verificación de integridad de la versión</b></summary>

```bash
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify <archive-file> -R Graywizard888/MovieBox-Tui-Mastered
```

</details>

<details>
<summary><b>Desinstalar</b></summary>

Vuelve a ejecutar el comando de instalación (`curl ... | bash` o `irm ... | iex`) y selecciona `2) Uninstall`.

O mediante gestor de paquetes:
```bash
cargo uninstall moviebox-tui    # Cargo
```

</details>

## Inicio rápido

```bash
moviebox-tui
```

- Escribe cualquier título para buscar, presiona `Enter` para reproducir.
- Presiona `?` dentro de la interfaz para ver los atajos, o escribe `/settings` para abrir las preferencias.

## Documentación

Las guías completas y referencias de arquitectura están disponibles en [**GitHub**](https://github.com/Graywizard888/MovieBox-Tui-Mastered/tree/main/docs) o en el directorio [`docs/`](docs/):

| Guía | Descripción |
| :--- | :--- |
| [Teclado y Controles](docs/controls.md) | Atajos de teclado, navegación vim, búsqueda y comandos |
| [Configuración](docs/config.md) | Opciones de configuración, personalización de temas y variables de entorno |
| [Proveedores de Contenido](docs/providers.md) | Scrapers nativos (MovieBox, 4KHDHub, Dramachi, BDIX) |
| [Complementos de Stremio](docs/addons-mode.md) | Instalación de addons comunitarios, URLs de manifiestos y resolución de streams |
| [Reproductores Multimedia](docs/players.md) | Detección de reproductores, opciones de inicio y aceleración por hardware |
| [TV en vivo e IPTV](docs/tv-mode.md) | Importación de listas M3U, búsqueda de canales y streaming en vivo |
| [Descargas por Lotes](docs/downloads.md) | Descargas multisegmento por rangos HTTP con soporte de pausa y reanudación |

## Contribuir

Cualquier contribución es bienvenida. Revisa [CONTRIBUTING.md](CONTRIBUTING.md) antes de enviar pull requests.

Reporta errores o sugiere nuevas funciones a través de [GitHub Issues](https://github.com/Graywizard888/MovieBox-Tui-Mastered/issues).

<details>
<summary><b>Apoyo opcional</b></summary>
<div id="apoyo-opcional" tabindex="-1"></div>

Si deseas apoyar el desarrollo continuo directamente:

| Red / Criptomoneda | Dirección |
| :--- | :--- |
| **USDT (TRC20)** | `TL4yW73qmbKZpBWwbEFgjBpwVkPDFTkJgV` |
| **Bitcoin (BTC)** | `3MEAtqtRWrQBhnaMi3Zuf5nt2efNUS2LUQ` |
| **Ethereum / EVM** | `0x7ea20d5fa29d87f33195f5a3b211ff94038d794c` |
| **Solana (SOL)** | `6ctm5WFv73MNywoCKAz3xK72yizSspHa72rFNygooU6` |

</details>

## Privacidad

MovieBox-TUI contiene cero telemetría, análisis o seguimiento de usuarios. Todo el historial de búsqueda, marcadores y archivos de configuración permanecen estrictamente en tu sistema de archivos local.

## Licencia

Distribuido bajo licencias [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE).

## Descargo de responsabilidad

Este proyecto no aloja ni almacena ningún contenido multimedia. Es un cliente independiente para reproducir transmisiones de video disponibles públicamente. Los usuarios son responsables de cumplir con las leyes de sus respectivos países.
