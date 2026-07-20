use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use dji_log_parser::layout::auxiliary::Department;
use dji_log_parser::layout::details::Details;
use dji_log_parser::DJILog;

#[derive(thiserror::Error, Debug)]
pub enum ConvertError {
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Parse(#[from] dji_log_parser::Error),
    #[error("parser crashed on this file (likely corrupt or truncated)")]
    Panic,
    #[error("no valid GPS points found in this log")]
    NoTrack,
    #[error("failed to write KMZ: {0}")]
    Kmz(String),
}

pub struct FlightMeta {
    pub display_name: String,
    pub model: String,
    pub aircraft_sn: String,
    pub aircraft_name: String,
    pub battery_sn: String,
    pub start_time: DateTime<Utc>,
}

impl FlightMeta {
    fn from_details(details: &Details, input_path: &Path) -> Self {
        let file_stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Flight")
            .to_string();
        let display_name = if details.aircraft_name.trim().is_empty() {
            file_stem
        } else {
            details.aircraft_name.clone()
        };
        Self {
            display_name,
            model: format!("{:?}", details.product_type),
            aircraft_sn: details.aircraft_sn.clone(),
            aircraft_name: details.aircraft_name.clone(),
            battery_sn: details.battery_sn.clone(),
            start_time: details.start_time,
        }
    }
}

pub struct FlightStats {
    pub duration_secs: f64,
    pub total_distance_m: f64,
    pub max_altitude_m: f64,
    pub max_speed_ms: f64,
}

impl FlightStats {
    /// `details.total_distance` (the DJI-firmware-computed header field) was
    /// found to be unreliable against a real log (reported 2m on a flight
    /// that clearly covered ~2.4km) — compute distance, max altitude, and
    /// max speed directly from the frame/point data instead. Only duration
    /// is trusted from the header, since it matched ground truth exactly.
    fn compute(details: &Details, frames: &[dji_log_parser::frame::Frame], points: &[(f64, f64, f64)]) -> Self {
        let total_distance_m = points
            .windows(2)
            .map(|w| haversine_m((w[0].1, w[0].0), (w[1].1, w[1].0)))
            .sum();

        let max_altitude_m = points
            .iter()
            .map(|(_, _, alt)| *alt)
            .fold(0.0_f64, f64::max);

        let max_speed_ms = frames
            .iter()
            .map(|f| {
                let x = f.osd.x_speed as f64;
                let y = f.osd.y_speed as f64;
                if x.is_finite() && y.is_finite() {
                    (x * x + y * y).sqrt()
                } else {
                    0.0
                }
            })
            .fold(0.0_f64, f64::max);

        Self {
            duration_secs: details.total_time,
            total_distance_m,
            max_altitude_m,
            max_speed_ms,
        }
    }
}

/// Great-circle distance between two (lat, lon) points, in meters.
fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let d_lat = lat2 - lat1;
    let d_lon = lon2 - lon1;
    let h = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

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
) -> dji_log_parser::Result<Vec<Vec<dji_log_parser::keychain::KeychainFeaturePoint>>> {
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

    let parser = match std::panic::catch_unwind(move || DJILog::from_bytes(bytes)) {
        Ok(Ok(parser)) => parser,
        Ok(Err(e)) => return Err(ConvertError::Parse(e)),
        Err(_) => return Err(ConvertError::Panic),
    };

    let keychains = if parser.version >= 13 {
        Some(fetch_keychains_with_fallback(&parser, api_key)?)
    } else {
        None
    };

    let frames = match std::panic::catch_unwind(AssertUnwindSafe(|| parser.frames(keychains))) {
        Ok(Ok(frames)) => frames,
        Ok(Err(e)) => return Err(ConvertError::Parse(e)),
        Err(_) => return Err(ConvertError::Panic),
    };

    // Flight path only: filter to finite, in-range, non-placeholder GPS
    // points and keep them in original (chronological) frame order.
    let points: Vec<(f64, f64, f64)> = frames
        .iter()
        .filter(|f| {
            f.osd.latitude.is_finite()
                && f.osd.longitude.is_finite()
                && f.osd.height.is_finite()
                && !(f.osd.latitude.abs() < 1e-6 && f.osd.longitude.abs() < 1e-6)
                && f.osd.latitude.abs() <= 90.0
                && f.osd.longitude.abs() <= 180.0
        })
        .map(|f| (f.osd.longitude, f.osd.latitude, f.osd.height as f64))
        .collect();

    if points.is_empty() {
        return Err(ConvertError::NoTrack);
    }

    let meta = FlightMeta::from_details(&parser.details, input_path);
    let stats = FlightStats::compute(&parser.details, &frames, &points);

    let kml = crate::kml::build_kml(&meta, &stats, &points);

    let file_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("flight");
    let output_path = output_dir.join(file_stem).with_extension("kmz");
    crate::kml::write_kmz(&output_path, &kml).map_err(ConvertError::Kmz)?;

    Ok(ConvertOutcome {
        output_path,
        point_count: points.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_real_sample_log() {
        let fixture = Path::new("tests/fixtures/sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }
        let out_dir = std::env::temp_dir();
        let api_key = crate::config::resolve_api_key();
        let outcome = convert_file(fixture, &out_dir, &api_key).expect("conversion should succeed");
        assert!(outcome.point_count > 0);
        assert!(outcome.output_path.exists());
        let _ = std::fs::remove_file(&outcome.output_path);
    }
}
