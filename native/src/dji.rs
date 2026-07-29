use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use dji_log_parser::keychain::KeychainFeaturePoint;
use dji_log_parser::layout::auxiliary::Department;
use dji_log_parser::DJILog;

pub use dji2kmz_core::dji::ConvertError;
use dji2kmz_core::dji::FlightData;

/// One flight's parsed/converted result, before it's written to disk — the
/// caller decides the final destination directory (which depends on this
/// flight's own `local_date`, not known until after parsing), so writing
/// is a separate step from `parse_and_convert`.
pub struct ConvertedFlight {
    /// Filename (no extension, no collision-dedup suffix yet) — same
    /// format the web build uses.
    pub base_name: String,
    /// `MM-DD-YYYY`, the local date embedded in the original filename
    /// (falling back to the parsed UTC start time) — used both for the
    /// destination date-folder and the pilot log's Date column.
    pub local_date: String,
    /// "HH:MM" local takeoff/landing, for the pilot log.
    pub takeoff: String,
    pub landing: String,
    pub kml: String,
    /// This flight's raw parsed data, so the caller can accumulate it
    /// across a batch for a per-date merged KMZ and pilot log rows.
    pub flight_data: FlightData,
    pub point_count: usize,
}

/// Fetch the decryption keychain for a v13+ log. Tries the standard
/// (log-determined) department first; some third-party-app-recorded logs
/// only succeed against DJI's API when forced to the DJIFly department, so
/// retry with that override on failure before giving up.
fn fetch_keychains_with_fallback(
    parser: &DJILog,
    api_key: &str,
) -> dji_log_parser::Result<Vec<Vec<KeychainFeaturePoint>>> {
    match parser.fetch_keychains(api_key) {
        Ok(keychains) => Ok(keychains),
        Err(_) => {
            let request =
                parser.keychains_request_with_custom_params(Some(Department::DJIFly), None)?;
            request.fetch(api_key, None)
        }
    }
}

/// Appends " (2)", " (3)", ... if `{base_name}.kmz` already exists in
/// `output_dir`, so two flights that land on the same computed name in one
/// batch run don't silently overwrite each other.
fn unique_output_path(output_dir: &Path, base_name: &str) -> PathBuf {
    let candidate = output_dir.join(base_name).with_extension("kmz");
    if !candidate.exists() {
        return candidate;
    }
    let mut n = 2;
    loop {
        let candidate = output_dir
            .join(format!("{base_name} ({n})"))
            .with_extension("kmz");
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Parse one DJI `.txt` flight log and build its flight-path KML, named
/// from the flight's local date/time (parsed from the original filename)
/// and the name of `input_root` (the originally selected folder — the
/// "location"/project). Does not write anything to disk — see
/// `write_kmz_file`. One bad/corrupt file must never abort a batch run, so
/// parsing is wrapped in `catch_unwind` — the underlying crate can panic
/// on truncated/malformed input.
///
/// `input_root` is deliberately NOT the same as `input_path`'s immediate
/// parent: `input_path` may sit one level deeper, in a pilot subfolder
/// (`{input_root}/{Pilot Name}/file.txt`), and the location name must
/// always come from `input_root` itself, never from whatever folder
/// happens to directly contain the file.
pub fn parse_and_convert(
    input_path: &Path,
    input_root: &Path,
    api_key: &str,
) -> Result<ConvertedFlight, ConvertError> {
    let bytes = std::fs::read(input_path)?;

    let parser = match std::panic::catch_unwind(move || dji2kmz_core::dji::parse_bytes(bytes)) {
        Ok(Ok(parser)) => parser,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(ConvertError::Panic),
    };

    let keychains = if parser.version >= 13 {
        Some(fetch_keychains_with_fallback(&parser, api_key).map_err(ConvertError::Parse)?)
    } else {
        None
    };

    let file_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("flight");

    let relative = input_path
        .strip_prefix(input_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let pilot = dji2kmz_core::naming::extract_pilot_name(&relative).unwrap_or_default();

    let flight_data = match std::panic::catch_unwind(AssertUnwindSafe(|| {
        dji2kmz_core::dji::extract_flight_data(&parser, keychains, file_stem, &pilot)
    })) {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(ConvertError::Panic),
    };

    let (meta, stats, points) = &flight_data;
    let point_count = points.len();
    let kml = dji2kmz_core::kml::build_kml(meta, stats, points);

    let original_filename = input_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(file_stem);
    let folder_name = input_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Flight_Logs");

    let (base_name, local_date) =
        dji2kmz_core::naming::individual_filename(original_filename, meta.start_time, folder_name);
    let times = dji2kmz_core::naming::pilot_log_times(original_filename, meta.start_time, stats.duration_secs);

    Ok(ConvertedFlight {
        base_name,
        local_date,
        takeoff: times.takeoff,
        landing: times.landing,
        kml,
        flight_data,
        point_count,
    })
}

/// Writes `kml` to `{output_dir}/{base_name}.kmz`, creating `output_dir`
/// if needed and deduping against a same-named file already there.
pub fn write_kmz_file(output_dir: &Path, base_name: &str, kml: &str) -> Result<PathBuf, ConvertError> {
    std::fs::create_dir_all(output_dir)?;
    let output_path = unique_output_path(output_dir, base_name);
    let file = std::fs::File::create(&output_path)?;
    dji2kmz_core::kml::write_kmz(file, kml).map_err(ConvertError::Kmz)?;
    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_real_sample_log() {
        let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let fixture = fixtures_dir.join("sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }
        let api_key = crate::config::resolve_api_key();
        let converted = parse_and_convert(&fixture, &fixtures_dir, &api_key)
            .expect("conversion should succeed");
        assert!(converted.point_count > 0);

        let out_dir = std::env::temp_dir().join("dji2kmz_dji_test_output");
        let output_path = write_kmz_file(&out_dir, &converted.base_name, &converted.kml)
            .expect("write should succeed");
        assert!(output_path.exists());
        let _ = std::fs::remove_dir_all(&out_dir);
    }
}
