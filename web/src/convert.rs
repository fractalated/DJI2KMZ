use dji2kmz_core::dji::ConvertError;

/// Parses one DJI `.txt` file's bytes, fetches its decryption keychain (if
/// needed) through `proxy_url` rather than DJI's endpoint directly — a
/// direct browser call to DJI would be blocked by CORS — and returns the
/// finished `.kmz` file's bytes.
pub async fn convert(
    bytes: Vec<u8>,
    file_stem: &str,
    api_key: &str,
    proxy_url: &str,
) -> Result<Vec<u8>, ConvertError> {
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

    let result = dji2kmz_core::dji::finish_conversion(&parser, keychains, file_stem)?;

    let cursor = dji2kmz_core::kml::write_kmz(std::io::Cursor::new(Vec::new()), &result.kml)
        .map_err(ConvertError::Kmz)?;
    Ok(cursor.into_inner())
}
