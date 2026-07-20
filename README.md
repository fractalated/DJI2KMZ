# DJI2KMZ

A small, standalone desktop app that batch-converts DJI drone flight logs
(`.txt`) into `.kmz` flight-path files for viewing in Google Earth and
similar tools.

No server, no install beyond the app itself, no account, no internet
access required except for one thing: decrypting newer/encrypted DJI logs
(firmware-version 13+) requires one HTTPS call to DJI's own servers to
fetch a decryption key. Older logs need no network access at all.

## What it does

1. Pick a folder containing DJI `.txt` flight logs (any other file types in
   that folder are ignored).
2. Pick a folder to save the converted files into.
3. Click Convert. Each `.txt` becomes one `.kmz` file with the flight's GPS
   path, plus a description box containing drone model, serial numbers,
   start time, duration, distance, max altitude, and max speed.
4. When done, open the output folder directly from the app, or copy its
   path.

One bad/corrupt log file is skipped (and reported) rather than stopping
the whole batch.

## Download

Grab a pre-built binary from the [Releases page](../../releases) — no
Rust installation or build step needed. Download and double-click:

- Windows: `dji2kmz-windows-x64.exe`
- macOS (Apple Silicon): `dji2kmz-macos-arm64`

## Building from source

Requires [Rust](https://rustup.rs/).

```bash
cargo build --release
```

The binary will be at `target/release/dji2kmz` (or `dji2kmz.exe` on
Windows).

## Configuration

DJI's decryption API key is bundled with a working default — no setup
needed. To use your own key instead (e.g. for higher rate limits), set the
`DJI2KMZ_API_KEY` environment variable before launching.
