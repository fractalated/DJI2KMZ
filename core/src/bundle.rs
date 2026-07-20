use std::io::{Cursor, Write};

/// Bundles multiple already-converted `.kmz` files into one outer `.zip`
/// for a single download in the web build. Uses `Stored` (no
/// re-compression) since each `.kmz` payload is already deflate-compressed
/// internally — re-deflating it would just burn CPU for no size benefit.
pub struct ZipBundle {
    writer: zip::ZipWriter<Cursor<Vec<u8>>>,
}

impl Default for ZipBundle {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipBundle {
    pub fn new() -> Self {
        Self {
            writer: zip::ZipWriter::new(Cursor::new(Vec::new())),
        }
    }

    pub fn add_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        self.writer
            .start_file(name, options)
            .map_err(|e| e.to_string())?;
        self.writer.write_all(bytes).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        let cursor = self.writer.finish().map_err(|e| e.to_string())?;
        Ok(cursor.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_multiple_files_into_one_zip() {
        let mut bundle = ZipBundle::new();
        bundle.add_file("a.kmz", b"fake kmz bytes a").unwrap();
        bundle.add_file("b.kmz", b"fake kmz bytes b").unwrap();
        let bytes = bundle.finish().unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 2);
        assert!(archive.by_name("a.kmz").is_ok());
        assert!(archive.by_name("b.kmz").is_ok());
    }
}
