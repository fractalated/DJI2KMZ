mod convert;

use dji2kmz_core::dji::FlightData;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The bundled default DJI SDK key, exposed so the page can pre-fill it.
#[wasm_bindgen]
pub fn default_api_key() -> String {
    dji2kmz_core::config::DEFAULT_API_KEY.to_string()
}

fn to_js_error(e: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&e.to_string()).into()
}

fn strip_txt_extension(filename: &str) -> &str {
    if filename.len() >= 4 && filename[filename.len() - 4..].eq_ignore_ascii_case(".txt") {
        &filename[..filename.len() - 4]
    } else {
        filename
    }
}

/// Bundles multiple already-converted `.kmz` files into one outer `.zip`
/// for a single download, since browsers can't write to an arbitrary
/// chosen folder the way the native app does.
#[wasm_bindgen]
pub struct ZipBundle(dji2kmz_core::bundle::ZipBundle);

#[wasm_bindgen]
impl ZipBundle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ZipBundle {
        ZipBundle(dji2kmz_core::bundle::ZipBundle::new())
    }

    pub fn add_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), JsValue> {
        self.0.add_file(name, bytes).map_err(to_js_error)
    }

    /// Consumes the bundle — wasm-bindgen invalidates the JS-side handle
    /// automatically after this by-value method runs, matching the "can't
    /// add more files after finishing" intent.
    pub fn finish(self) -> Result<Uint8Array, JsValue> {
        let bytes = self.0.finish().map_err(to_js_error)?;
        Ok(Uint8Array::from(bytes.as_slice()))
    }
}

impl Default for ZipBundle {
    fn default() -> Self {
        Self::new()
    }
}

/// One flight's conversion result: its computed output filename (no
/// extension — same format the native app uses) and its individual
/// `.kmz` bytes. A small struct instead of two separate return values
/// since wasm-bindgen exports return exactly one value.
#[wasm_bindgen]
pub struct ConvertedFlight {
    filename: String,
    bytes: Uint8Array,
}

#[wasm_bindgen]
impl ConvertedFlight {
    #[wasm_bindgen(getter)]
    pub fn filename(&self) -> String {
        self.filename.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Uint8Array {
        self.bytes.clone()
    }
}

/// Converts each DJI `.txt` log in a batch AND accumulates their raw data
/// so a single combined multi-flight `.kmz` can be built once the batch is
/// done — mirrors the native app's per-run behavior of producing both the
/// individual files and one merged file. Kept entirely in Rust so no
/// structured flight data (`Frame`, `FlightMeta`, ...) ever needs to cross
/// the JS boundary, matching `ZipBundle`'s existing design.
#[wasm_bindgen]
pub struct MergedKmlBuilder {
    folder_name: String,
    flights: Vec<FlightData>,
    dates: Vec<String>,
    // Same name each flight's individual .kmz gets, so its layer in the
    // merged KMZ is identifiable — meta.display_name alone is often
    // identical across every flight from the same aircraft. Doesn't
    // reflect any collision-dedup suffix JS applies after add_and_convert
    // returns (a known, accepted minor gap — collisions are rare).
    names: Vec<String>,
}

#[wasm_bindgen]
impl MergedKmlBuilder {
    /// `folder_name` is the name of the folder the user selected (from
    /// `webkitRelativePath`'s first path segment) — used both to name
    /// individual files consistently with the native app and to title the
    /// eventual merged file.
    #[wasm_bindgen(constructor)]
    pub fn new(folder_name: String) -> MergedKmlBuilder {
        MergedKmlBuilder {
            folder_name,
            flights: Vec::new(),
            dates: Vec::new(),
            names: Vec::new(),
        }
    }

    /// Parses, decrypts (via `proxy_url` if the log needs it), and converts
    /// one DJI `.txt` file's bytes. Returns that flight's computed
    /// filename plus its individual `.kmz` bytes (for the caller's outer
    /// `ZipBundle`), and internally accumulates the raw flight data for
    /// `finish()`'s merged KMZ.
    ///
    /// `bytes` must be owned (not borrowed) — wasm-bindgen disallows
    /// borrowed references as parameters to `async fn` exports, since it
    /// can't prove the borrow outlives the awaited call.
    pub async fn add_and_convert(
        &mut self,
        bytes: Vec<u8>,
        original_filename: String,
        api_key: String,
        proxy_url: String,
    ) -> Result<ConvertedFlight, JsValue> {
        let file_stem = strip_txt_extension(&original_filename).to_string();

        let (kml, flight_data) =
            convert::convert_for_merge(bytes, &file_stem, &api_key, &proxy_url)
                .await
                .map_err(to_js_error)?;

        let (meta, _, _) = &flight_data;
        let (filename, local_date) = dji2kmz_core::naming::individual_filename(
            &original_filename,
            meta.start_time,
            &self.folder_name,
        );
        self.dates.push(local_date);
        self.names.push(filename.clone());

        let cursor = dji2kmz_core::kml::write_kmz(std::io::Cursor::new(Vec::new()), &kml)
            .map_err(to_js_error)?;
        self.flights.push(flight_data);

        Ok(ConvertedFlight {
            filename,
            bytes: Uint8Array::from(cursor.into_inner().as_slice()),
        })
    }

    /// The merged file's title/filename (no extension), computed from the
    /// folder name and every date accumulated so far. Call before
    /// `finish()`, which consumes the builder.
    pub fn title(&self) -> String {
        dji2kmz_core::naming::merged_title(&self.folder_name, &self.dates)
    }

    /// Consumes the builder, returning the merged multi-flight `.kmz`
    /// bytes. Errors if no flights were successfully added.
    pub fn finish(self) -> Result<Uint8Array, JsValue> {
        if self.flights.is_empty() {
            return Err(to_js_error("no flights to merge"));
        }
        let title = dji2kmz_core::naming::merged_title(&self.folder_name, &self.dates);
        let kml = dji2kmz_core::kml::build_merged_kml(&title, &self.names, &self.flights);
        let cursor = dji2kmz_core::kml::write_kmz(std::io::Cursor::new(Vec::new()), &kml)
            .map_err(to_js_error)?;
        Ok(Uint8Array::from(cursor.into_inner().as_slice()))
    }
}
