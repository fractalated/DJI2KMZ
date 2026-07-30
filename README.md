# DJI2KMZ

Batch-converts DJI drone flight logs (`.txt`) into `.kmz` flight-path files
for viewing in Google Earth and similar tools, plus two read-only web
pages for browsing the results. Available these ways — pick whichever
fits:

- **[Try the converter in your browser](https://fractalated.github.io/DJI2KMZ/)** —
  no download, no install, nothing to trust or get flagged by antivirus/
  SmartScreen (there's no executable at all, just a web page).
- **Native Windows app** — download and double-click, no install step.
- **Native macOS app** — download and double-click, no install step.
- **[Flight log viewer](https://fractalated.github.io/DJI2KMZ/viewer/)** —
  a separate, read-only page for browsing already-converted `.kmz` files
  (e.g. on a shared network drive), with a Project/Date sidebar and flight
  paths rendered over satellite imagery. See [below](#flight-log-viewer).
- **[Pilot logbook](https://fractalated.github.io/DJI2KMZ/logbook/)** — a
  separate, read-only page listing pilots with hours flown. Currently
  secondary to the `Pilot Logs/Flight Log.xlsx` spreadsheet described
  above — see [below](#pilot-logbook).

The converter (all three ways of running it) shares the exact same
conversion logic and is built from the same commit on every release, so
they never drift out of sync.

No account, no server you have to run, no data uploaded anywhere except
one thing: decrypting newer/encrypted DJI logs (firmware version 13+)
requires a small HTTPS call to DJI's own servers to fetch a decryption
key. Older logs need no network access at all, on any version.

## What it does

1. Pick DJI `.txt` flight logs (native apps and web: point at a folder —
   everything except `.txt` files in it is ignored).
2. Pick a destination folder — both native apps and the web version now
   write directly into it (the web version uses the File System Access
   API's read-write folder access, so nothing is zipped or downloaded
   anymore).
3. Click Convert. Each `.txt` becomes one `.kmz` file with the flight's GPS
   path, plus a description box containing drone model, serial numbers,
   pilot (see below), start time, duration, distance, max altitude, and
   max speed. Everything lands in a structured layout under the
   destination folder:

   ```
   {destination}/
     KMZs/
       {Project Name}/            <- from the selected folder's name, filler
                                      words ("Flight Logs" etc.) stripped
         2026-06-15/
           06-15-2026_08-18_....kmz   <- one per flight
           06-15-2026_09-30_....kmz
         2026-06-16/
           ...
     Pilot Logs/
       Flight Log.xlsx             <- one tab per pilot, appended to on
                                      every import run (see below)
   ```
4. Native apps: open the destination folder directly from the app, or
   copy its path.

One bad/corrupt log file is skipped (and reported) rather than stopping
the whole batch.

## Pilot log spreadsheet

Every import run appends to `Pilot Logs/Flight Log.xlsx` inside the
destination folder — one worksheet per pilot, columns Date, Takeoff Time,
Landing Time, Flight Time, and Aircraft Type. It's read back and appended
to (not overwritten) on every subsequent run, so it builds into a full
logbook over time regardless of whether you're using the native app or
the web version — both write to the exact same file format via the same
shared logic. Flights with no pilot subfolder (see below) land under an
"Unknown Pilot" tab.

**Pilot attribution:** DJI flight logs don't record who was flying — there's
no such field anywhere in the format. If you organize your `.txt` files
into a subfolder named after the pilot before converting
(`{Location}/{Pilot Name}/*.txt`, one level under the folder you select),
that name is picked up automatically and baked into the `.kmz`'s
description as `Pilot: <name>`. Files placed directly in the location
folder (no pilot subfolder) still convert normally — pilot is just absent,
not an error, and shows as "Unknown Pilot" in the logbook.

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

- **One-time setup only:** click **Connect** and select
  `O:\Flight Logs Output (Do Not Put Files Here)\KMZs` (not the
  destination folder itself — the viewer only ever looks at `KMZs`, never
  `Pilot Logs`). Your browser remembers this afterward, so this is the
  only time anyone using the page needs to know where the files actually
  live — every visit after that just shows a list of projects, with no
  folder or connection choice exposed anywhere in the page. Requires
  Chrome or Edge (uses the File System Access API, not supported in
  Firefox/Safari). This location is fixed in the page itself, not a user
  setting — if it ever needs to change, that's a code change, not
  something end users can do.
- The sidebar is a flat list of **projects** — one per subfolder of
  `KMZs`, derived entirely from folder names, so browsing is instant even
  with a lot of data; nothing gets opened/parsed until you click into it.
  Clicking a project **opens every flight it has** (across all its
  dates) at once — already checked and drawn on the map, no manual
  selecting required — with **Select all**/**Deselect all** above that
  project's own list for quickly narrowing down from there. A **Clear
  All Selections** button above the whole project list hides everything
  currently shown, handy when switching between projects. (`.kmz` files
  sitting directly in `KMZs/` from before this folder structure existed
  still show up, grouped under an "Other" section at the bottom rather
  than disappearing.)
- Each flight's checkbox works like Google Earth's Places panel — check
  one to draw its path on the map, uncheck to hide it.
- Satellite imagery is Esri World Imagery (free, no API key). Map
  rendering is [MapLibre GL JS](https://maplibre.org/).
- View-only by design — no editing, no writing back to the source files.

## Pilot logbook

**Currently secondary to the `Pilot Logs/Flight Log.xlsx` spreadsheet**
described [above](#pilot-log-spreadsheet) — kept working, but not the
primary way to check pilot hours for now. A future update will make this
page the primary web-based view once the Excel output has proven itself.

A separate, read-only page at
[`/logbook/`](https://fractalated.github.io/DJI2KMZ/logbook/) — a digital
version of a traditional paper pilot logbook: hours flown, aircraft type,
dates, and locations, no flight tracks (that's what the viewer is for).

- Same connect flow as the viewer (point it at the same folder of
  converted `.kmz` files; nothing is ever uploaded anywhere).
- Lists every pilot found, each with total hours and flight count.
  Clicking a pilot shows their full table — date, aircraft, location,
  duration — sorted newest-first, with an hours-by-aircraft-type
  breakdown above it.
- Unlike the viewer, this page reads every location's `.kmz` up front
  (pilot isn't in any filename the way date/location are, so there's no
  way to build the pilot list without opening file content) — still just
  one file read per location where a merged `.kmz` exists, not per
  individual flight.
- Flights with no pilot subfolder used at conversion time (including
  anything converted before this feature existed) are grouped under
  "Unknown Pilot" rather than dropped.

## Project structure

A Cargo workspace, so the native apps and the web converter share the
exact same conversion/KML logic rather than duplicating it. The viewer and
logbook pages are plain JavaScript with no Rust/wasm dependency at all —
they only ever need to understand this project's own known, simple
`.kmz`/KML shape.

- `core/` — platform-agnostic parsing, GPS filtering, stats, KML/KMZ
  building, pilot-subfolder extraction, destination folder/date naming,
  and the pilot log spreadsheet (`pilotlog.rs`, via `rust_xlsxwriter` for
  writing and `calamine` for reading an existing workbook back in before
  appending — both wasm32-compatible, so this logic runs identically in
  the native app and the web build). No GUI, no HTTP.
- `native/` — the desktop app (`egui`/`eframe`). Package name `dji2kmz`.
- `web/` — a `wasm-bindgen` crate exposing the same conversion logic to
  the browser, plus the static `index.html` converter frontend and two
  read-only pages that share code (below) rather than duplicating it:
  `viewer/` (`map.js`, `viewer.js`) and `logbook/` (`logbook.js`).
- `web/static/shared/` — File System Access API + IndexedDB persistence
  (`fs.js`), `.kmz` unzip + KML parsing (`kml.js`), and folder-grouping/
  date logic (`grouping.js`), used identically by both the viewer and the
  logbook.
- `worker/` — a small Cloudflare Worker that relays the DJI decryption API
  call for the web converter. Browsers can't call DJI's API directly (it
  doesn't allow cross-origin requests), so this exists purely as a CORS
  workaround — it's a dumb relay with no secrets or logic of its own. The
  viewer and logbook don't need this at all — neither ever talks to DJI.

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
