mod convert;

use dji2kmz_core::dji::FlightData;
use dji2kmz_core::pilotlog::PilotLogRow;
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

/// Cleans a raw selected-folder name into a destination *project* folder
/// name — filler words ("Flight Logs" etc.) stripped, spaces kept (unlike
/// the underscore-joined names used for individual/merged `.kmz`
/// filenames). Used to name `KMZs/{project_name}/` on the destination.
#[wasm_bindgen]
pub fn clean_project_name(folder_name: &str) -> String {
    dji2kmz_core::naming::clean_project_name(folder_name)
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

/// One flight's conversion result: its computed output filename (no
/// extension — same format the native app uses), its individual `.kmz`
/// bytes, and the destination date-folder (`YYYY-MM-DD`) it belongs
/// under — so the caller can route it into `KMZs/{project}/{date}/`
/// without re-deriving anything.
#[wasm_bindgen]
pub struct ConvertedFlight {
    filename: String,
    bytes: Uint8Array,
    date_folder: String,
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

    #[wasm_bindgen(getter)]
    pub fn date_folder(&self) -> String {
        self.date_folder.clone()
    }
}

/// Converts each DJI `.txt` log in a batch AND accumulates their raw data
/// so the pilot log spreadsheet can be built once the whole batch is
/// done. Kept entirely in Rust so no structured flight data (`Frame`,
/// `FlightMeta`, ...) ever needs to cross the JS boundary.
#[wasm_bindgen]
pub struct ConversionBatch {
    folder_name: String,
    flights: Vec<FlightData>,
    dates: Vec<String>, // MM-DD-YYYY, one per entry in `flights`
    takeoffs: Vec<String>,
    landings: Vec<String>,
}

#[wasm_bindgen]
impl ConversionBatch {
    /// `folder_name` is the name of the folder the user selected (from
    /// `webkitRelativePath`'s first path segment) — used to name
    /// individual files consistently with the native app.
    #[wasm_bindgen(constructor)]
    pub fn new(folder_name: String) -> ConversionBatch {
        ConversionBatch {
            folder_name,
            flights: Vec::new(),
            dates: Vec::new(),
            takeoffs: Vec::new(),
            landings: Vec::new(),
        }
    }

    /// Parses, decrypts (via `proxy_url` if the log needs it), and converts
    /// one DJI `.txt` file's bytes. Returns that flight's computed
    /// filename, individual `.kmz` bytes, and destination date-folder (for
    /// the caller's `KMZs/{project}/{date}/` routing), and internally
    /// accumulates the raw flight data for `update_pilot_log()`.
    ///
    /// `bytes` must be owned (not borrowed) — wasm-bindgen disallows
    /// borrowed references as parameters to `async fn` exports, since it
    /// can't prove the borrow outlives the awaited call.
    ///
    /// `relative_path` is the file's path *within the selected folder*
    /// (e.g. `"John_Smith/DJIFlightRecord_....txt"` for a file in a pilot
    /// subfolder, or just `"DJIFlightRecord_....txt"` for one placed
    /// directly in the selected folder) — not just the bare filename, so
    /// pilot attribution can be derived the same way the native app does.
    pub async fn add_and_convert(
        &mut self,
        bytes: Vec<u8>,
        relative_path: String,
        api_key: String,
        proxy_url: String,
    ) -> Result<ConvertedFlight, JsValue> {
        let original_filename = relative_path
            .rsplit('/')
            .next()
            .unwrap_or(&relative_path)
            .to_string();
        let file_stem = strip_txt_extension(&original_filename).to_string();
        let pilot = dji2kmz_core::naming::extract_pilot_name(&relative_path).unwrap_or_default();

        let (kml, flight_data) =
            convert::convert_for_merge(bytes, &file_stem, &pilot, &api_key, &proxy_url)
                .await
                .map_err(to_js_error)?;

        let (meta, stats, _) = &flight_data;
        let (filename, local_date) = dji2kmz_core::naming::individual_filename(
            &original_filename,
            meta.start_time,
            &self.folder_name,
        );
        let times = dji2kmz_core::naming::pilot_log_times(&original_filename, meta.start_time, stats.duration_secs);

        self.dates.push(local_date.clone());
        self.takeoffs.push(times.takeoff);
        self.landings.push(times.landing);

        let cursor = dji2kmz_core::kml::write_kmz(std::io::Cursor::new(Vec::new()), &kml)
            .map_err(to_js_error)?;
        self.flights.push(flight_data);

        Ok(ConvertedFlight {
            filename,
            bytes: Uint8Array::from(cursor.into_inner().as_slice()),
            date_folder: dji2kmz_core::naming::date_folder_name(&local_date),
        })
    }

    /// Builds the updated Pilot Log workbook: `existing_bytes` (the
    /// current `Pilot Logs/Flight Log.xlsx` on the destination, if any)
    /// with every flight accumulated in this batch appended to the right
    /// pilot's sheet. Every field needed is already held by this builder
    /// (pilot/aircraft/times), so nothing but the existing workbook's
    /// bytes needs to cross the JS boundary.
    pub fn update_pilot_log(&self, existing_bytes: Option<Vec<u8>>) -> Result<Uint8Array, JsValue> {
        let rows: Vec<PilotLogRow> = (0..self.flights.len())
            .map(|i| {
                let (meta, stats, _) = &self.flights[i];
                PilotLogRow {
                    pilot: meta.pilot.clone(),
                    date_mm_dd_yyyy: self.dates[i].clone(),
                    takeoff: self.takeoffs[i].clone(),
                    landing: self.landings[i].clone(),
                    duration_secs: stats.duration_secs,
                    aircraft: meta.model.clone(),
                }
            })
            .collect();

        let bytes = dji2kmz_core::pilotlog::update_pilot_log(existing_bytes.as_deref(), &rows)
            .map_err(to_js_error)?;
        Ok(Uint8Array::from(bytes.as_slice()))
    }
}
