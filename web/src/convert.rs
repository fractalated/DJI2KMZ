use dji2kmz_core::dji::{ConvertError, FlightData};

/// Parses one DJI `.txt` file's bytes, fetches its decryption keychain (if
/// needed) through `proxy_url` rather than DJI's endpoint directly — a
/// direct browser call to DJI would be blocked by CORS — and returns both
/// this flight's rendered KML string (for its individual `.kmz`) and its
/// raw parsed data (for the caller to also accumulate into a merged
/// multi-flight KMZ).
pub async fn convert_for_merge(
    bytes: Vec<u8>,
    file_stem: &str,
    pilot: &str,
    api_key: &str,
    proxy_url: &str,
) -> Result<(String, FlightData), ConvertError> {
    let parser = dji2kmz_core::dji::parse_bytes(bytes)?;

    let keychains = match dji2kmz_core::dji::keychain_request(&parser)? {
        Some(request) => Some(
            request
                .fetch_async(api_key, Some(proxy_url))
                .await
                .map_err(ConvertError::Parse)?,
        ),
        None => None,
    };

    let flight_data = dji2kmz_core::dji::extract_flight_data(&parser, keychains, file_stem, pilot)?;
    let (meta, stats, points) = &flight_data;
    let kml = dji2kmz_core::kml::build_kml(meta, stats, points);

    Ok((kml, flight_data))
}
