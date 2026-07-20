/// Bundled default DJI SDK key so encrypted (v13+) logs decrypt with zero
/// setup. Same publicly-known default used by the Open DroneLog project.
/// Override via the DJI2KMZ_API_KEY (or DJI_API_KEY) environment variable.
const DEFAULT_API_KEY: &str = "7860e0c278e44617fd4c64fd86cfeaa";

pub fn resolve_api_key() -> String {
    std::env::var("DJI2KMZ_API_KEY")
        .or_else(|_| std::env::var("DJI_API_KEY"))
        .unwrap_or_else(|_| DEFAULT_API_KEY.to_string())
}
