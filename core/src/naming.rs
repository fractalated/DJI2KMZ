use chrono::{DateTime, Utc};

/// Words stripped (whole-word, case-insensitive) from a folder name before
/// it's used in an output filename/title — e.g. "Midland Airport Flight
/// Logs" becomes "Midland Airport".
const FILLER_WORDS: &[&str] = &["flight", "flights", "log", "logs"];

/// Parses a DJI-style filename like
/// `DJIFlightRecord_2026-06-15_[08-18-13].txt` and returns
/// `("06-15-2026", "08-18")` — the drone's LOCAL date/time as recorded in
/// the filename (seconds dropped), not the parsed UTC `start_time`, which
/// can differ by several hours depending on timezone. Returns `None` if the
/// filename doesn't contain a `YYYY-MM-DD` segment followed by an
/// `[HH-MM-SS]` bracket — callers should fall back to formatting the
/// parsed UTC `start_time` instead in that case.
pub fn extract_local_date_time(filename: &str) -> Option<(String, String)> {
    let (year, month, day) = find_iso_date(filename)?;

    let bracket_start = filename.find('[')?;
    let bracket_end = filename.find(']')?;
    if bracket_end <= bracket_start {
        return None;
    }
    let bracket = &filename[bracket_start + 1..bracket_end];
    let mut parts = bracket.split('-');
    let hour = parts.next()?;
    let minute = parts.next()?;
    let _seconds = parts.next()?; // intentionally dropped
    if parts.next().is_some() {
        return None; // expected exactly HH-MM-SS
    }
    if !is_two_digit(hour) || !is_two_digit(minute) {
        return None;
    }

    Some((format!("{month}-{day}-{year}"), format!("{hour}-{minute}")))
}

/// Finds the first `YYYY-MM-DD`-shaped segment in a string split on `_`,
/// `/`, or `\`. Manual parsing rather than a `regex` dependency — the DJI
/// filename shape is fixed and simple enough not to justify the extra
/// crate.
fn find_iso_date(s: &str) -> Option<(&str, &str, &str)> {
    s.split(['_', '/', '\\']).find_map(parse_iso_date_segment)
}

fn parse_iso_date_segment(segment: &str) -> Option<(&str, &str, &str)> {
    let bytes = segment.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let (year, month, day) = (&segment[0..4], &segment[5..7], &segment[8..10]);
    if year.bytes().all(|b| b.is_ascii_digit())
        && month.bytes().all(|b| b.is_ascii_digit())
        && day.bytes().all(|b| b.is_ascii_digit())
    {
        Some((year, month, day))
    } else {
        None
    }
}

fn is_two_digit(s: &str) -> bool {
    s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit())
}

/// Strips filler words from a folder name and joins what's left with
/// underscores (Windows filenames can't contain spaces safely across every
/// tool, and this keeps the whole output filename consistently
/// space-free). Falls back to the original (uncleaned) name if stripping
/// filler words would remove everything.
pub fn clean_folder_name(folder_name: &str) -> String {
    let cleaned: Vec<&str> = folder_name
        .split_whitespace()
        .filter(|word| !FILLER_WORDS.contains(&word.to_lowercase().as_str()))
        .collect();

    if cleaned.is_empty() {
        folder_name.split_whitespace().collect::<Vec<_>>().join("_")
    } else {
        cleaned.join("_")
    }
}

/// Returns `("{MM-DD-YYYY}_{HH-MM}_{cleaned_folder_name}", "MM-DD-YYYY")`
/// — the individual flight's output filename (extension added by the
/// caller) and the date component alone, so a caller batching multiple
/// flights can accumulate dates for the merged KMZ's date-range title
/// without re-deriving them.
pub fn individual_filename(
    original_filename: &str,
    start_time_utc: DateTime<Utc>,
    folder_name: &str,
) -> (String, String) {
    let (date, time) = extract_local_date_time(original_filename).unwrap_or_else(|| {
        (
            start_time_utc.format("%m-%d-%Y").to_string(),
            start_time_utc.format("%H-%M").to_string(),
        )
    });
    let folder = clean_folder_name(folder_name);
    (format!("{date}_{time}_{folder}"), date)
}

/// `"{cleaned_folder_name}_Flight_Logs_{date_or_range}"` for the combined
/// multi-flight KMZ. `dates_mm_dd_yyyy` should be the same `MM-DD-YYYY`
/// strings produced alongside each flight's individual filename, for
/// consistency between the two.
pub fn merged_title(folder_name: &str, dates_mm_dd_yyyy: &[String]) -> String {
    let folder = clean_folder_name(folder_name);
    let date_part = date_range_label(dates_mm_dd_yyyy);
    format!("{folder}_Flight_Logs_{date_part}")
}

/// Single date if every flight falls on the same day, else
/// `"{MM}-{DD}--{MM}-{DD}-{YYYY}"` spanning the earliest to latest date.
fn date_range_label(dates_mm_dd_yyyy: &[String]) -> String {
    let mut sortable: Vec<(String, &str)> = dates_mm_dd_yyyy
        .iter()
        .filter_map(|d| {
            let parts: Vec<&str> = d.split('-').collect();
            if parts.len() != 3 {
                return None;
            }
            // "YYYYMMDD" sort key from an "MM-DD-YYYY" display string.
            Some((format!("{}{}{}", parts[2], parts[0], parts[1]), d.as_str()))
        })
        .collect();
    sortable.sort();

    match (sortable.first(), sortable.last()) {
        (Some((_, first)), Some((_, last))) if first == last => first.to_string(),
        (Some((_, first)), Some((_, last))) => {
            let first_month_day = &first[..5]; // "MM-DD"
            format!("{first_month_day}--{last}")
        }
        _ => "Unknown_Date".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn extracts_local_date_time_from_a_real_dji_filename() {
        let result = extract_local_date_time("DJIFlightRecord_2026-06-15_[08-18-13].txt");
        assert_eq!(result, Some(("06-15-2026".to_string(), "08-18".to_string())));
    }

    #[test]
    fn returns_none_for_a_filename_without_the_expected_shape() {
        assert_eq!(extract_local_date_time("random_export.txt"), None);
        assert_eq!(extract_local_date_time("Flight_2026-06-15.txt"), None); // no bracket
    }

    #[test]
    fn falls_back_to_utc_start_time_for_an_unrecognized_filename() {
        let utc_time = chrono::Utc.with_ymd_and_hms(2026, 6, 15, 14, 18, 13).unwrap();
        let (name, date) = individual_filename("renamed_export.txt", utc_time, "Midland Airport");
        assert_eq!(name, "06-15-2026_14-18_Midland_Airport");
        assert_eq!(date, "06-15-2026");
    }

    #[test]
    fn strips_filler_words_case_insensitively() {
        assert_eq!(clean_folder_name("Midland Airport Flight Logs"), "Midland_Airport");
        assert_eq!(clean_folder_name("midland airport FLIGHT LOGS"), "midland_airport");
        assert_eq!(clean_folder_name("Site Survey Log"), "Site_Survey");
    }

    #[test]
    fn falls_back_to_original_name_when_every_word_is_filler() {
        assert_eq!(clean_folder_name("Flight Logs"), "Flight_Logs");
    }

    #[test]
    fn builds_individual_filename_from_bracket_time() {
        let utc_time = chrono::Utc.with_ymd_and_hms(2026, 6, 15, 14, 18, 13).unwrap();
        let (name, date) = individual_filename(
            "DJIFlightRecord_2026-06-15_[08-18-13].txt",
            utc_time,
            "Midland Airport Flight Logs",
        );
        // Local time (08-18) from the bracket, NOT the UTC start_time (14-18).
        assert_eq!(name, "06-15-2026_08-18_Midland_Airport");
        assert_eq!(date, "06-15-2026");
    }

    #[test]
    fn builds_merged_title_for_a_single_day() {
        let dates = vec!["06-15-2026".to_string(), "06-15-2026".to_string()];
        assert_eq!(
            merged_title("Midland Airport Flight Logs", &dates),
            "Midland_Airport_Flight_Logs_06-15-2026"
        );
    }

    #[test]
    fn builds_merged_title_for_a_multi_day_range() {
        let dates = vec![
            "06-18-2026".to_string(),
            "06-15-2026".to_string(),
            "06-16-2026".to_string(),
        ];
        assert_eq!(
            merged_title("Midland Airport Flight Logs", &dates),
            "Midland_Airport_Flight_Logs_06-15--06-18-2026"
        );
    }
}
