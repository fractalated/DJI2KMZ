# DJI2KMZ

Batch-converts DJI drone flight logs (`.txt`) into `.kmz` flight-path files
for viewing in Google Earth and similar tools. Available three ways —
pick whichever fits:

- **[Try it in your browser](https://fractalated.github.io/DJI2KMZ/)** —
  no download, no install, nothing to trust or get flagged by antivirus/
  SmartScreen (there's no executable at all, just a web page).
- **Native Windows app** — download and double-click, no install step.
- **Native macOS app** — download and double-click, no install step.

All three share the exact same conversion logic and are built from the
same commit on every release, so they never drift out of sync.

No account, no server you have to run, no data uploaded anywhere except
one thing: decrypting newer/encrypted DJI logs (firmware version 13+)
requires a small HTTPS call to DJI's own servers to fetch a decryption
key. Older logs need no network access at all, on any version.

## What it does

1. Pick DJI `.txt` flight logs (native apps: point at a folder, everything
   else in it is ignored; web version: select one or more files directly).
2. Native apps: pick a folder to save into. Web version: converted files
   are bundled into one `.zip` and downloaded, since browsers can't write
   to an arbitrary chosen folder.
3. Click Convert. Each `.txt` becomes one `.kmz` file with the flight's GPS
   path, plus a description box containing drone model, serial numbers,
   start time, duration, distance, max altitude, and max speed.
4. Native apps: open the output folder directly from the app, or copy its
   path.

One bad/corrupt log file is skipped (and reported) rather than stopping
the whole batch.

## Download (native apps)

Grab a pre-built binary from the [Releases page](../../releases) — no
Rust installation or build step needed. Download and double-click:

- Windows: `dji2kmz-windows-x64.exe`
- macOS (Apple Silicon): `dji2kmz-macos-arm64`

> **Note:** these aren't code-signed (that costs money this project
> doesn't have), so Windows SmartScreen will likely flag the `.exe` as
> from an unrecognized publisher on first run. If that's a blocker for
> you — e.g. on a work machine — use the [web version](https://fractalated.github.io/DJI2KMZ/)
> instead, which has no such warning since there's no executable at all.

## Project structure

A Cargo workspace, so the native apps and the web version share the exact
same conversion/KML logic rather than duplicating it:

- `core/` — platform-agnostic parsing, GPS filtering, stats, KML/KMZ
  building. No GUI, no HTTP, no wasm-specific code.
- `native/` — the desktop app (`egui`/`eframe`). Package name `dji2kmz`.
- `web/` — a `wasm-bindgen` crate exposing the same conversion logic to
  the browser, plus the static `index.html` frontend.
- `worker/` — a small Cloudflare Worker that relays the DJI decryption API
  call for the web version. Browsers can't call DJI's API directly (it
  doesn't allow cross-origin requests), so this exists purely as a CORS
  workaround — it's a dumb relay with no secrets or logic of its own.

## Building from source

Requires [Rust](https://rustup.rs/).

**Native app:**
```bash
cargo build --release -p dji2kmz
```
The binary will be at `target/release/dji2kmz` (or `dji2kmz.exe` on
Windows). Note the `-p dji2kmz` — a plain `cargo build --release` also
tries to build the `web` crate for your native target, which fails (it
depends on wasm-only APIs).

**Web version:**
```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build web --target web --release --out-dir pkg
```
Then serve `web/static/index.html` alongside the generated
`web/pkg/dji2kmz_web.js` and `web/pkg/dji2kmz_web_bg.wasm` from any static
file server (must be served over http(s), not opened via `file://`).

## Configuration

DJI's decryption API key is bundled with a working default — no setup
needed. Native apps: override it via the `DJI2KMZ_API_KEY` (or
`DJI_API_KEY`) environment variable. Web version: edit the value in the
API Key field before converting.
