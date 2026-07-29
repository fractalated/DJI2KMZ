use std::collections::{BTreeMap, HashSet};
use std::io::Cursor;

use calamine::{Data, Reader, Xlsx, open_workbook_from_rs};
use rust_xlsxwriter::Workbook;

use crate::kml::format_duration;

const HEADERS: [&str; 5] = ["Date", "Takeoff Time", "Landing Time", "Flight Time", "Aircraft Type"];

/// One flight's row in the pilot log spreadsheet. `duration_secs` is
/// formatted into the "Flight Time" column the same way the KML
/// description's Duration field is, so the two never disagree.
pub struct PilotLogRow {
    pub pilot: String,
    pub date_mm_dd_yyyy: String,
    pub takeoff: String,
    pub landing: String,
    pub duration_secs: f64,
    pub aircraft: String,
}

#[derive(Clone)]
struct SheetRow {
    date_mm_dd_yyyy: String,
    takeoff: String,
    landing: String,
    flight_time: String,
    aircraft: String,
}

/// "MM-DD-YYYY" -> "YYYYMMDD", for chronological sorting (same trick as
/// `naming::date_range_label`).
fn sort_key(date_mm_dd_yyyy: &str) -> String {
    match date_mm_dd_yyyy.split('-').collect::<Vec<_>>().as_slice() {
        [mm, dd, yyyy] => format!("{yyyy}{mm}{dd}"),
        _ => date_mm_dd_yyyy.to_string(),
    }
}

/// Reads every pilot's existing rows out of a previously-generated
/// workbook, so a new import run can append to it instead of starting
/// over. Returns an empty map (rather than an error) for anything that
/// doesn't parse as a workbook — for an ever-growing appended file the
/// safest behavior is to never lose already-imported flights by failing
/// loudly on an unreadable/corrupt existing file, not to abort the run.
fn read_existing(bytes: &[u8]) -> BTreeMap<String, Vec<SheetRow>> {
    let mut sheets = BTreeMap::new();
    let Ok(mut workbook) = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(bytes)) else {
        return sheets;
    };
    for name in workbook.sheet_names() {
        let Ok(range) = workbook.worksheet_range(&name) else {
            continue;
        };
        let rows: Vec<SheetRow> = range
            .rows()
            .skip(1) // header row
            .filter(|row| !row.is_empty())
            .map(|row| {
                let cell = |i: usize| row.get(i).map(Data::to_string).unwrap_or_default();
                SheetRow {
                    date_mm_dd_yyyy: cell(0),
                    takeoff: cell(1),
                    landing: cell(2),
                    flight_time: cell(3),
                    aircraft: cell(4),
                }
            })
            .collect();
        sheets.insert(name, rows);
    }
    sheets
}

/// Excel worksheet names can't exceed 31 characters, can't contain
/// `[ ] : * ? / \`, can't be blank, and can't start/end with an
/// apostrophe. Pilot names are free text taken from a folder name, so
/// sanitize rather than error out.
fn sanitize_sheet_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '_' } else { c })
        .collect();
    let truncated: String = cleaned.trim_matches('\'').chars().take(31).collect();
    let trimmed = truncated.trim();
    if trimmed.is_empty() { "Unknown Pilot".to_string() } else { trimmed.to_string() }
}

/// Appends " (2)", " (3)", ... (staying within Excel's 31-character sheet
/// name limit) if `base` collides with a name already used in this
/// workbook — e.g. two pilot folder names that only differ past the
/// truncation point. Mirrors `native`'s `unique_output_path` collision
/// handling for `.kmz` filenames.
fn unique_sheet_name(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let suffix = format!(" ({n})");
        let max_base_len = 31usize.saturating_sub(suffix.chars().count());
        let candidate = format!("{}{suffix}", base.chars().take(max_base_len).collect::<String>());
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// Reads `existing_bytes` (if this is a rerun against a workbook already
/// on disk), appends `new_rows`, and returns the finished workbook's
/// bytes — one worksheet per pilot (alphabetical), each pilot's rows
/// sorted chronologically. `existing_bytes` being `None` (no prior file)
/// is not an error — the run simply starts a fresh workbook.
pub fn update_pilot_log(existing_bytes: Option<&[u8]>, new_rows: &[PilotLogRow]) -> Result<Vec<u8>, String> {
    let mut sheets = existing_bytes.map(read_existing).unwrap_or_default();

    for row in new_rows {
        let pilot = if row.pilot.trim().is_empty() {
            "Unknown Pilot".to_string()
        } else {
            row.pilot.clone()
        };
        sheets.entry(pilot).or_default().push(SheetRow {
            date_mm_dd_yyyy: row.date_mm_dd_yyyy.clone(),
            takeoff: row.takeoff.clone(),
            landing: row.landing.clone(),
            flight_time: format_duration(row.duration_secs),
            aircraft: row.aircraft.clone(),
        });
    }

    let mut workbook = Workbook::new();
    let mut used_names = HashSet::new();
    let mut pilots: Vec<&String> = sheets.keys().collect();
    pilots.sort();

    for pilot in pilots {
        let mut rows = sheets[pilot].clone();
        rows.sort_by(|a, b| sort_key(&a.date_mm_dd_yyyy).cmp(&sort_key(&b.date_mm_dd_yyyy)));

        let sheet_name = unique_sheet_name(&sanitize_sheet_name(pilot), &mut used_names);
        let sheet = workbook.add_worksheet();
        sheet.set_name(sheet_name).map_err(|e| e.to_string())?;

        for (col, header) in HEADERS.iter().enumerate() {
            sheet.write(0, col as u16, *header).map_err(|e| e.to_string())?;
        }
        for (i, row) in rows.iter().enumerate() {
            let r = (i + 1) as u32;
            sheet.write(r, 0, &row.date_mm_dd_yyyy).map_err(|e| e.to_string())?;
            sheet.write(r, 1, &row.takeoff).map_err(|e| e.to_string())?;
            sheet.write(r, 2, &row.landing).map_err(|e| e.to_string())?;
            sheet.write(r, 3, &row.flight_time).map_err(|e| e.to_string())?;
            sheet.write(r, 4, &row.aircraft).map_err(|e| e.to_string())?;
        }
    }

    workbook.save_to_buffer().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pilot: &str, date: &str, takeoff: &str, landing: &str, aircraft: &str) -> PilotLogRow {
        PilotLogRow {
            pilot: pilot.to_string(),
            date_mm_dd_yyyy: date.to_string(),
            takeoff: takeoff.to_string(),
            landing: landing.to_string(),
            duration_secs: 1530.0, // 25m 30s
            aircraft: aircraft.to_string(),
        }
    }

    #[test]
    fn builds_a_fresh_workbook_with_one_sheet_per_pilot() {
        let rows = vec![
            row("Jane Doe", "06-15-2026", "08:18", "08:43", "M350"),
            row("John Smith", "06-16-2026", "09:00", "09:25", "Mavic3"),
        ];
        let bytes = update_pilot_log(None, &rows).expect("should build a workbook");

        let sheets = read_existing(&bytes);
        assert_eq!(sheets.len(), 2);
        assert!(sheets.contains_key("Jane Doe"));
        assert!(sheets.contains_key("John Smith"));
        let jane = &sheets["Jane Doe"];
        assert_eq!(jane.len(), 1);
        assert_eq!(jane[0].date_mm_dd_yyyy, "06-15-2026");
        assert_eq!(jane[0].takeoff, "08:18");
        assert_eq!(jane[0].flight_time, "25m 30s");
        assert_eq!(jane[0].aircraft, "M350");
    }

    #[test]
    fn appends_to_an_existing_workbook_across_two_calls() {
        let first = update_pilot_log(None, &[row("Jane Doe", "06-15-2026", "08:18", "08:43", "M350")])
            .expect("first run should succeed");

        let second_rows = vec![row("Jane Doe", "06-16-2026", "09:00", "09:25", "M350")];
        let second = update_pilot_log(Some(&first), &second_rows).expect("second run should succeed");

        let sheets = read_existing(&second);
        assert_eq!(sheets.len(), 1);
        let jane = &sheets["Jane Doe"];
        assert_eq!(jane.len(), 2, "second run should append, not replace, the first run's row");
        assert_eq!(jane[0].date_mm_dd_yyyy, "06-15-2026");
        assert_eq!(jane[1].date_mm_dd_yyyy, "06-16-2026");
    }

    #[test]
    fn rows_are_sorted_chronologically_within_a_pilot_regardless_of_insertion_order() {
        let rows = vec![
            row("Jane Doe", "06-20-2026", "08:00", "08:30", "M350"),
            row("Jane Doe", "06-10-2026", "08:00", "08:30", "M350"),
        ];
        let bytes = update_pilot_log(None, &rows).expect("should build a workbook");
        let sheets = read_existing(&bytes);
        let jane = &sheets["Jane Doe"];
        assert_eq!(jane[0].date_mm_dd_yyyy, "06-10-2026");
        assert_eq!(jane[1].date_mm_dd_yyyy, "06-20-2026");
    }

    #[test]
    fn empty_pilot_name_is_grouped_under_unknown_pilot() {
        let bytes = update_pilot_log(None, &[row("", "06-15-2026", "08:18", "08:43", "M350")])
            .expect("should build a workbook");
        let sheets = read_existing(&bytes);
        assert!(sheets.contains_key("Unknown Pilot"));
    }

    #[test]
    fn sanitizes_illegal_characters_out_of_pilot_sheet_names() {
        assert_eq!(sanitize_sheet_name("John/Smith"), "John_Smith");
        assert_eq!(sanitize_sheet_name("'Jane'"), "Jane");
        let long_name = "A".repeat(50);
        assert_eq!(sanitize_sheet_name(&long_name).len(), 31);
    }

    #[test]
    fn dedupes_colliding_sheet_names() {
        let mut used = HashSet::new();
        assert_eq!(unique_sheet_name("Jane", &mut used), "Jane");
        assert_eq!(unique_sheet_name("Jane", &mut used), "Jane (2)");
    }
}
