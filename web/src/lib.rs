mod convert;

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The bundled default DJI SDK key, exposed so the page can pre-fill it.
#[wasm_bindgen]
pub fn default_api_key() -> String {
    dji2kmz_core::config::DEFAULT_API_KEY.to_string()
}

fn to_js_error(e: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&e.to_string()).into()
}

/// Converts one DJI `.txt` log's bytes into `.kmz` bytes.
///
/// `bytes` must be owned (not borrowed) — wasm-bindgen disallows borrowed
/// references as parameters to `async fn` exports, since it can't prove
/// the borrow outlives the awaited call. `proxy_url` is the Cloudflare
/// Worker relay used to reach DJI's decryption API without hitting a
/// browser CORS wall.
#[wasm_bindgen]
pub async fn convert_log(
    bytes: Vec<u8>,
    file_stem: String,
    api_key: String,
    proxy_url: String,
) -> Result<Uint8Array, JsValue> {
    let out = convert::convert(bytes, &file_stem, &api_key, &proxy_url)
        .await
        .map_err(to_js_error)?;
    Ok(Uint8Array::from(out.as_slice()))
}

/// Bundles multiple converted `.kmz` files into one `.zip` for a single
/// download, since browsers can't write to an arbitrary chosen folder the
/// way the native app does.
#[wasm_bindgen]
pub struct ZipBundle(dji2kmz_core::bundle::ZipBundle);

#[wasm_bindgen]
impl ZipBundle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ZipBundle {
        ZipBundle(dji2kmz_core::bundle::ZipBundle::new())
    }

    pub fn add_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), JsValue> {
        self.0.add_file(name, bytes).map_err(to_js_error)
    }

    /// Consumes the bundle — wasm-bindgen invalidates the JS-side handle
    /// automatically after this by-value method runs, matching the "can't
    /// add more files after finishing" intent.
    pub fn finish(self) -> Result<Uint8Array, JsValue> {
        let bytes = self.0.finish().map_err(to_js_error)?;
        Ok(Uint8Array::from(bytes.as_slice()))
    }
}

impl Default for ZipBundle {
    fn default() -> Self {
        Self::new()
    }
}
