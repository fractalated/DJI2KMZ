use std::io::{Seek, Write};

use crate::dji::{FlightMeta, FlightStats};

pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Escape a literal `]]>` so it can't prematurely close a CDATA section.
fn escape_cdata(s: &str) -> String {
    s.replace("]]>", "]]]]><![CDATA[>")
}

fn format_duration(total_secs: f64) -> String {
    let secs = total_secs.round().max(0.0) as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
    }
}

/// Build a minimal KML: one Document, one line Style, one Placemark with a
/// LineString flight path. All other flight data goes in the description
/// box rather than as separate KML structures.
pub fn build_kml(meta: &FlightMeta, stats: &FlightStats, points: &[(f64, f64, f64)]) -> String {
    let name = escape_xml(&meta.display_name);

    let coords = points
        .iter()
        .map(|(lon, lat, alt)| format!("{lon},{lat},{alt}"))
        .collect::<Vec<_>>()
        .join(" ");

    let raw_description = format!(
        "Drone Model: {}\nAircraft Serial: {}\nAircraft Name: {}\nBattery Serial: {}\nStart Time: {}\nDuration: {}\nDistance: {:.0} m\nMax Altitude: {:.1} m\nMax Speed: {:.1} m/s",
        non_empty(&meta.model),
        non_empty(&meta.aircraft_sn),
        non_empty(&meta.aircraft_name),
        non_empty(&meta.battery_sn),
        meta.start_time.to_rfc3339(),
        format_duration(stats.duration_secs),
        stats.total_distance_m,
        stats.max_altitude_m,
        stats.max_speed_ms,
    );
    let description = escape_cdata(&raw_description);

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2">
  <Document>
    <name>{name}</name>
    <Style id="flightPath">
      <LineStyle>
        <color>ff0080ff</color>
        <width>3</width>
      </LineStyle>
    </Style>
    <Placemark>
      <name>{name}</name>
      <description><![CDATA[{description}]]></description>
      <styleUrl>#flightPath</styleUrl>
      <LineString>
        <altitudeMode>relativeToGround</altitudeMode>
        <coordinates>{coords}</coordinates>
      </LineString>
    </Placemark>
  </Document>
</kml>"#
    )
}

fn non_empty(s: &str) -> &str {
    if s.trim().is_empty() {
        "Unknown"
    } else {
        s
    }
}

/// Write `kml` as the `doc.kml` entry of a `.kmz` (zip) archive to `writer`.
/// Generic over `Write + Seek` so the same function serves both
/// `std::fs::File` (native, writing directly to disk) and
/// `std::io::Cursor<Vec<u8>>` (web, building the file entirely in memory
/// before handing the bytes to JS for download) — same body either way.
pub fn write_kmz<W: Write + Seek>(writer: W, kml: &str) -> Result<W, String> {
    let mut zip = zip::ZipWriter::new(writer);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("doc.kml", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(kml.as_bytes()).map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Cursor;

    fn synthetic_points() -> Vec<(f64, f64, f64)> {
        vec![
            (-102.4419, 31.5396, 10.0),
            (-102.4420, 31.5397, 15.0),
            (-102.4421, 31.5398, 20.0),
        ]
    }

    #[test]
    fn builds_valid_kml_with_correct_coordinate_order() {
        let meta = FlightMeta {
            display_name: "Test Flight".into(),
            model: "Matrice350RTK".into(),
            aircraft_sn: "SN123".into(),
            aircraft_name: "Ninja".into(),
            battery_sn: "BAT123".into(),
            start_time: chrono::Utc.with_ymd_and_hms(2026, 6, 15, 8, 18, 13).unwrap(),
        };
        let stats = FlightStats {
            duration_secs: 1769.5,
            total_distance_m: 2444.1,
            max_altitude_m: 37.5,
            max_speed_ms: 14.19,
        };
        let points = synthetic_points();
        let kml = build_kml(&meta, &stats, &points);

        assert!(kml.contains("<coordinates>"));
        // lon,lat,alt order — longitude (-102.44...) must come before latitude (31.53...)
        assert!(kml.contains("-102.4419,31.5396,10"));
        assert!(kml.contains("Matrice350RTK"));
        assert!(kml.contains("relativeToGround"));
    }

    #[test]
    fn writes_a_valid_kmz_zip() {
        let meta = FlightMeta {
            display_name: "Test".into(),
            model: "Test".into(),
            aircraft_sn: "".into(),
            aircraft_name: "".into(),
            battery_sn: "".into(),
            start_time: chrono::Utc::now(),
        };
        let stats = FlightStats {
            duration_secs: 60.0,
            total_distance_m: 100.0,
            max_altitude_m: 10.0,
            max_speed_ms: 5.0,
        };
        let kml = build_kml(&meta, &stats, &synthetic_points());
        let cursor = write_kmz(Cursor::new(Vec::new()), &kml).expect("write_kmz should succeed");
        let bytes = cursor.into_inner();
        assert!(!bytes.is_empty());

        // Confirm it round-trips as a valid zip with a doc.kml entry
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let entry = archive.by_name("doc.kml").expect("doc.kml entry should exist");
        assert!(entry.size() > 0);
    }
}
