use chrono::{DateTime, Duration, NaiveTime, Utc};

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

/// Splits a folder name into words with filler words removed. Falls back
/// to every word (uncleaned) if stripping filler words would remove
/// everything — a folder literally named "Flight Logs" should still
/// produce something, not an empty name.
fn strip_filler_words(folder_name: &str) -> Vec<&str> {
    let cleaned: Vec<&str> = folder_name
        .split_whitespace()
        .filter(|word| !FILLER_WORDS.contains(&word.to_lowercase().as_str()))
        .collect();

    if cleaned.is_empty() {
        folder_name.split_whitespace().collect()
    } else {
        cleaned
    }
}

/// Strips filler words from a folder name and joins what's left with
/// underscores (Windows filenames can't contain spaces safely across every
/// tool, and this keeps the whole output filename consistently
/// space-free). Used for individual/merged `.kmz` *filenames*.
pub fn clean_folder_name(folder_name: &str) -> String {
    strip_filler_words(folder_name).join("_")
}

/// Same filler-word stripping as `clean_folder_name`, but joined with
/// spaces instead of underscores — for use as an actual destination
/// *folder* name (e.g. `KMZs/{project_name}/`), where spaces are fine and
/// underscores would just look wrong next to a human-typed project name.
pub fn clean_project_name(folder_name: &str) -> String {
    strip_filler_words(folder_name).join(" ")
}

/// `"MM-DD-YYYY"` -> `"YYYY-MM-DD"`, a sortable per-day destination
/// subfolder name. Falls back to the input unchanged if it isn't shaped
/// like `MM-DD-YYYY` (shouldn't happen given callers always pass a string
/// produced by `individual_filename` or `pilot_log_times`).
pub fn date_folder_name(mm_dd_yyyy: &str) -> String {
    match mm_dd_yyyy.split('-').collect::<Vec<_>>().as_slice() {
        [mm, dd, yyyy] => format!("{yyyy}-{mm}-{dd}"),
        _ => mm_dd_yyyy.to_string(),
    }
}

/// One flight's local takeoff/landing times and date, for the pilot log
/// spreadsheet.
pub struct PilotLogTimes {
    pub date_mm_dd_yyyy: String,
    /// "HH:MM"
    pub takeoff: String,
    /// "HH:MM" — takeoff + duration. Wraps past midnight for the rare
    /// overnight flight (`NaiveTime` addition wraps modulo 24h rather than
    /// panicking) — an accepted display-only edge case.
    pub landing: String,
}

/// Derives the pilot log's date/takeoff/landing from the same local
/// date/time `individual_filename` already extracts from the original
/// filename (falling back to the parsed UTC `start_time` for a filename
/// that doesn't match DJI's usual shape), so the spreadsheet's date always
/// agrees with the destination date-folder a flight was written into.
pub fn pilot_log_times(
    original_filename: &str,
    start_time_utc: DateTime<Utc>,
    duration_secs: f64,
) -> PilotLogTimes {
    let (date_mm_dd_yyyy, takeoff) = extract_local_date_time(original_filename)
        .map(|(date, hh_mm)| (date, hh_mm.replace('-', ":")))
        .unwrap_or_else(|| {
            (
                start_time_utc.format("%m-%d-%Y").to_string(),
                start_time_utc.format("%H:%M").to_string(),
            )
        });

    let landing = takeoff
        .split_once(':')
        .and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .and_then(|(h, m)| NaiveTime::from_hms_opt(h, m, 0))
        .map(|takeoff_time| {
            (takeoff_time + Duration::seconds(duration_secs.round() as i64))
                .format("%H:%M")
                .to_string()
        })
        .unwrap_or_else(|| takeoff.clone());

    PilotLogTimes { date_mm_dd_yyyy, takeoff, landing }
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

/// Given a file's path relative to the selected/location folder (POSIX
/// "/"-separated — e.g. "John_Smith/DJIFlightRecord_...txt" for a file in
/// a pilot subfolder, or just "DJIFlightRecord_...txt" for a file placed
/// directly in the location folder), returns the pilot subfolder name if
/// the file is nested exactly one level deep, or `None` if it sits
/// directly in the location folder (no pilot attribution — not an error).
/// Deeper nesting (rare) still returns just the immediate first-level
/// folder, ignoring anything past it.
pub fn extract_pilot_name(relative_path: &str) -> Option<String> {
    let mut parts = relative_path.split('/');
    let first = parts.next()?;
    parts.next()?; // nothing follows `first` => no subfolder, bail via `?`
    Some(first.to_string())
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
    fn extract_pilot_name_returns_none_when_file_sits_directly_in_the_location_folder() {
        assert_eq!(extract_pilot_name("DJIFlightRecord_2026-06-15_[08-18-13].txt"), None);
    }

    #[test]
    fn extract_pilot_name_returns_the_subfolder_when_nested_one_level() {
        assert_eq!(
            extract_pilot_name("John_Smith/DJIFlightRecord_2026-06-15_[08-18-13].txt"),
            Some("John_Smith".to_string())
        );
    }

    #[test]
    fn extract_pilot_name_returns_only_the_first_level_when_nested_deeper() {
        assert_eq!(
            extract_pilot_name("John_Smith/2026-06-15/DJIFlightRecord_...txt"),
            Some("John_Smith".to_string())
        );
    }

    #[test]
    fn cleans_project_name_with_spaces_not_underscores() {
        assert_eq!(clean_project_name("East Waddell Ranch Flight Logs"), "East Waddell Ranch");
        assert_eq!(clean_project_name("Flight Logs"), "Flight Logs");
    }

    #[test]
    fn converts_mm_dd_yyyy_to_a_sortable_date_folder_name() {
        assert_eq!(date_folder_name("06-15-2026"), "2026-06-15");
    }

    #[test]
    fn falls_back_to_input_for_an_unrecognized_date_folder_name() {
        assert_eq!(date_folder_name("20260615"), "20260615");
    }

    #[test]
    fn pilot_log_times_from_a_real_dji_filename() {
        let utc_time = chrono::Utc.with_ymd_and_hms(2026, 6, 15, 14, 18, 13).unwrap();
        // 90 minutes: 08:18 takeoff -> 09:48 landing.
        let times = pilot_log_times("DJIFlightRecord_2026-06-15_[08-18-13].txt", utc_time, 5400.0);
        assert_eq!(times.date_mm_dd_yyyy, "06-15-2026");
        assert_eq!(times.takeoff, "08:18");
        assert_eq!(times.landing, "09:48");
    }

    #[test]
    fn pilot_log_times_falls_back_to_utc_start_time_for_an_unrecognized_filename() {
        let utc_time = chrono::Utc.with_ymd_and_hms(2026, 6, 15, 14, 18, 13).unwrap();
        let times = pilot_log_times("renamed_export.txt", utc_time, 600.0);
        assert_eq!(times.date_mm_dd_yyyy, "06-15-2026");
        assert_eq!(times.takeoff, "14:18");
        assert_eq!(times.landing, "14:28");
    }

    #[test]
    fn pilot_log_times_wraps_landing_time_past_midnight() {
        let utc_time = chrono::Utc.with_ymd_and_hms(2026, 6, 15, 14, 18, 13).unwrap();
        // 23:50 takeoff + 20 minutes -> wraps to 00:10.
        let times = pilot_log_times("DJIFlightRecord_2026-06-15_[23-50-00].txt", utc_time, 1200.0);
        assert_eq!(times.takeoff, "23:50");
        assert_eq!(times.landing, "00:10");
    }
}
