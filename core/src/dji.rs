use chrono::{DateTime, Utc};
use dji_log_parser::keychain::{KeychainFeaturePoint, KeychainsRequest};
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
    fn from_details(details: &Details, file_stem: &str) -> Self {
        let display_name = if details.aircraft_name.trim().is_empty() {
            file_stem.to_string()
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

pub struct ConversionResult {
    pub kml: String,
    pub point_count: usize,
}

/// Parse raw `.txt` bytes into a `DJILog`. Platform-agnostic (pure
/// `binrw`-over-`Vec<u8>` parsing, works identically on native and wasm32).
/// Panics on truncated/corrupt input are the caller's responsibility to
/// guard against — native wraps this in `catch_unwind`; wasm relies on
/// wasm-bindgen converting a panic into a catchable JS exception instead,
/// since `catch_unwind` isn't reliable on stable wasm32.
pub fn parse_bytes(bytes: Vec<u8>) -> Result<DJILog, ConvertError> {
    DJILog::from_bytes(bytes).map_err(ConvertError::Parse)
}

/// Build the keychain request for a v13+ (encrypted) log, or `None` if the
/// log doesn't need one. This only builds the request — actually fetching
/// it (sync native `ureq` vs async wasm `fetch()` through a CORS proxy) is
/// platform-specific and lives outside `core`.
pub fn keychain_request(parser: &DJILog) -> Result<Option<KeychainsRequest>, ConvertError> {
    if parser.version < 13 {
        return Ok(None);
    }
    Ok(Some(parser.keychains_request().map_err(ConvertError::Parse)?))
}

/// Given a parsed log and its already-fetched keychains (if any), extract
/// the flight path, compute stats, and build the KML string. Platform-
/// agnostic — no file I/O, no HTTP.
pub fn finish_conversion(
    parser: &DJILog,
    keychains: Option<Vec<Vec<KeychainFeaturePoint>>>,
    file_stem: &str,
) -> Result<ConversionResult, ConvertError> {
    let frames = parser.frames(keychains).map_err(ConvertError::Parse)?;

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

    let meta = FlightMeta::from_details(&parser.details, file_stem);
    let stats = FlightStats::compute(&parser.details, &frames, &points);
    let kml = crate::kml::build_kml(&meta, &stats, &points);

    Ok(ConversionResult {
        kml,
        point_count: points.len(),
    })
}
