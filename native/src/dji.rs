use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use dji_log_parser::keychain::KeychainFeaturePoint;
use dji_log_parser::layout::auxiliary::Department;
use dji_log_parser::DJILog;

pub use dji2kmz_core::dji::ConvertError;

pub struct ConvertOutcome {
    pub output_path: PathBuf,
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

/// Parse one DJI `.txt` flight log and write its flight path to a `.kmz`
/// file in `output_dir`. One bad/corrupt file must never abort a batch run,
/// so parsing is wrapped in `catch_unwind` — the underlying crate can panic
/// on truncated/malformed input.
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
        Some(
            fetch_keychains_with_fallback(&parser, api_key)
                .map_err(ConvertError::Parse)?,
        )
    } else {
        None
    };

    let file_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("flight");

    let result = match std::panic::catch_unwind(AssertUnwindSafe(|| {
        dji2kmz_core::dji::finish_conversion(&parser, keychains, file_stem)
    })) {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(ConvertError::Panic),
    };

    let output_path = output_dir.join(file_stem).with_extension("kmz");
    let file = std::fs::File::create(&output_path)?;
    dji2kmz_core::kml::write_kmz(file, &result.kml).map_err(ConvertError::Kmz)?;

    Ok(ConvertOutcome {
        output_path,
        point_count: result.point_count,
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
