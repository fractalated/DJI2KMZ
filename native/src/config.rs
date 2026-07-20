pub fn resolve_api_key() -> String {
    std::env::var("DJI2KMZ_API_KEY")
        .or_else(|_| std::env::var("DJI_API_KEY"))
        .unwrap_or_else(|_| dji2kmz_core::config::DEFAULT_API_KEY.to_string())
}
