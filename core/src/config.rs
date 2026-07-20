/// Bundled default DJI SDK key so encrypted (v13+) logs decrypt with zero
/// setup. Same publicly-known default used by the Open DroneLog project.
/// Native allows overriding this via the DJI2KMZ_API_KEY/DJI_API_KEY
/// environment variables (see native/src/config.rs); the web build exposes
/// this same constant as the input field's default value.
pub const DEFAULT_API_KEY: &str = "7860e0c278e44617fd4c64fd86cfeaa";
