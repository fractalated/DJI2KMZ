use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use dji_log_parser::keychain::KeychainFeaturePoint;
use dji_log_parser::layout::auxiliary::Department;
use dji_log_parser::DJILog;

pub use dji2kmz_core::dji::ConvertError;
use dji2kmz_core::dji::FlightData;

pub struct ConvertOutcome {
    pub output_path: PathBuf,
    pub point_count: usize,
    /// This flight's raw parsed data, so the caller can accumulate it
    /// across a batch for the merged multi-flight KMZ.
    pub flight_data: FlightData,
    /// The `MM-DD-YYYY` local date used in `output_path`'s filename, so the
    /// caller can compute the merged KMZ's date range without re-deriving
    /// it from the original filename.
    pub local_date: String,
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

/// Parse one DJI `.txt` flight log and write its flight path to a `.kmz`
/// file in `output_dir`, named from the flight's local date/time (parsed
/// from the original filename) and the name of the folder `input_path`
/// lives in. One bad/corrupt file must never abort a batch run, so parsing
/// is wrapped in `catch_unwind` — the underlying crate can panic on
/// truncated/malformed input.
pub fn convert_file(
    input_path: &Path,
    output_dir: &Path,
    api_key: &str,
) -> Result<ConvertOutcome, ConvertError> {
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

    let flight_data = match std::panic::catch_unwind(AssertUnwindSafe(|| {
        dji2kmz_core::dji::extract_flight_data(&parser, keychains, file_stem)
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
    let folder_name = input_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("Flight_Logs");

    let (base_name, local_date) =
        dji2kmz_core::naming::individual_filename(original_filename, meta.start_time, folder_name);
    let output_path = unique_output_path(output_dir, &base_name);

    let file = std::fs::File::create(&output_path)?;
    dji2kmz_core::kml::write_kmz(file, &kml).map_err(ConvertError::Kmz)?;

    Ok(ConvertOutcome {
        output_path,
        point_count,
        flight_data,
        local_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_real_sample_log() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }
        let out_dir = std::env::temp_dir();
        let api_key = crate::config::resolve_api_key();
        let outcome = convert_file(&fixture, &out_dir, &api_key).expect("conversion should succeed");
        assert!(outcome.point_count > 0);
        assert!(outcome.output_path.exists());
        let _ = std::fs::remove_file(&outcome.output_path);
    }
}
