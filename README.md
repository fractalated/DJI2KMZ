# DJI2KMZ

Batch-converts DJI drone flight logs (`.txt`) into `.kmz` flight-path files
for viewing in Google Earth and similar tools, plus a read-only web viewer
for browsing the results. Available these ways — pick whichever fits:

- **[Try the converter in your browser](https://fractalated.github.io/DJI2KMZ/)** —
  no download, no install, nothing to trust or get flagged by antivirus/
  SmartScreen (there's no executable at all, just a web page).
- **Native Windows app** — download and double-click, no install step.
- **Native macOS app** — download and double-click, no install step.
- **[Flight log viewer](https://fractalated.github.io/DJI2KMZ/viewer/)** —
  a separate, read-only page for browsing already-converted `.kmz` files
  (e.g. on a shared network drive) sorted by date/location, with flight
  paths rendered over satellite imagery. See [below](#flight-log-viewer).

The converter (all three ways of running it) shares the exact same
conversion logic and is built from the same commit on every release, so
they never drift out of sync.

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

## Flight log viewer

A separate, read-only page at
[`/viewer/`](https://fractalated.github.io/DJI2KMZ/viewer/) for browsing
`.kmz` files this converter has already produced — e.g. a shared folder or
network drive everyone on a team has access to.

- Click **Choose Folder** and point it at the folder containing your
  converted `.kmz` files. The page reads them **directly from that
  folder in your browser** — nothing is ever uploaded anywhere, so it's
  safe to use with in-house-only data even though the page itself is
  publicly hosted. Requires Chrome or Edge (uses the File System Access
  API, not supported in Firefox/Safari). Your browser will remember the
  folder for next time (re-confirming access once per session).
- The sidebar lists one entry per subfolder, sorted by date — newest
  first — derived entirely from filenames, so browsing is instant even
  with a lot of data; nothing gets opened/parsed until you click into it.
- Each location's flights show as a checklist, same as Google Earth's
  Places panel — check one to draw its path on the map, uncheck to hide
  it. If a folder has a merged `.kmz` (produced when converting a whole
  folder at once), that's used automatically; otherwise every individual
  `.kmz` in that folder is loaded.
- Satellite imagery is Esri World Imagery (free, no API key). Map
  rendering is [MapLibre GL JS](https://maplibre.org/).
- View-only by design — no editing, no writing back to the source files.

## Project structure

A Cargo workspace, so the native apps and the web converter share the
exact same conversion/KML logic rather than duplicating it. The viewer is
plain JavaScript with no Rust/wasm dependency at all — it only ever needs
to understand this project's own known, simple `.kmz`/KML shape.

- `core/` — platform-agnostic parsing, GPS filtering, stats, KML/KMZ
  building. No GUI, no HTTP, no wasm-specific code.
- `native/` — the desktop app (`egui`/`eframe`). Package name `dji2kmz`.
- `web/` — a `wasm-bindgen` crate exposing the same conversion logic to
  the browser, plus the static `index.html` converter frontend and the
  `viewer/` page (`fs.js`, `grouping.js`, `kml.js`, `map.js`, `viewer.js`).
- `worker/` — a small Cloudflare Worker that relays the DJI decryption API
  call for the web converter. Browsers can't call DJI's API directly (it
  doesn't allow cross-origin requests), so this exists purely as a CORS
  workaround — it's a dumb relay with no secrets or logic of its own. The
  viewer doesn't need this at all — it never talks to DJI.

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
