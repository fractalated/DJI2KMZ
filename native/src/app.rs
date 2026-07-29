use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dji2kmz_core::pilotlog::PilotLogRow;

use crate::progress::{ProgressError, ProgressState, SharedProgress};

pub struct DjiKmzApp {
    input_folder: Option<PathBuf>,
    output_folder: Option<PathBuf>,
    progress: SharedProgress,
    api_key: String,
}

impl Default for DjiKmzApp {
    fn default() -> Self {
        Self {
            input_folder: None,
            output_folder: None,
            progress: Arc::new(Mutex::new(ProgressState::default())),
            api_key: crate::config::resolve_api_key(),
        }
    }
}

impl DjiKmzApp {
    fn start_conversion(&self) {
        let Some(input) = self.input_folder.clone() else {
            return;
        };
        let Some(output) = self.output_folder.clone() else {
            return;
        };
        let api_key = self.api_key.clone();
        let progress = self.progress.clone();

        // One level of recursion: {input}/*.txt (no pilot subfolder) plus
        // {input}/{Pilot Name}/*.txt (pilot subfolder). Deliberately not
        // unbounded recursive walking — that's the viewer's job, not the
        // converter's; this matches exactly the location -> optional
        // pilot-subfolder -> files shape the naming convention expects.
        fn is_txt_file(path: &std::path::Path) -> bool {
            path.is_file()
                && path
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("txt"))
                    .unwrap_or(false)
        }

        let mut files: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&input).into_iter().flatten().filter_map(|e| e.ok()) {
            let path = entry.path();
            if is_txt_file(&path) {
                files.push(path);
            } else if path.is_dir() {
                files.extend(
                    std::fs::read_dir(&path)
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| is_txt_file(p)),
                );
            }
        }
        files.sort();

        {
            let mut state = progress.lock().unwrap();
            *state = ProgressState {
                total: files.len(),
                completed: 0,
                current_file: None,
                done: false,
                running: true,
                errors: Vec::new(),
                output_dir: Some(output.display().to_string()),
            };
        }

        std::thread::spawn(move || {
            let folder_name_raw = input
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Flight_Logs")
                .to_string();
            let project_name = dji2kmz_core::naming::clean_project_name(&folder_name_raw);
            let kmzs_root = output.join("KMZs").join(&project_name);
            let mut pilot_rows: Vec<PilotLogRow> = Vec::new();

            for file in files {
                let name = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();

                if let Ok(mut state) = progress.lock() {
                    state.current_file = Some(name.clone());
                }

                match crate::dji::parse_and_convert(&file, &input, &api_key) {
                    Ok(converted) => {
                        let date_folder = dji2kmz_core::naming::date_folder_name(&converted.local_date);
                        let dest_dir = kmzs_root.join(&date_folder);

                        match crate::dji::write_kmz_file(&dest_dir, &converted.base_name, &converted.kml) {
                            Ok(_) => {
                                let (pilot, aircraft, duration_secs) = {
                                    let (meta, stats, _) = &converted.flight_data;
                                    (meta.pilot.clone(), meta.model.clone(), stats.duration_secs)
                                };
                                pilot_rows.push(PilotLogRow {
                                    pilot,
                                    date_mm_dd_yyyy: converted.local_date.clone(),
                                    takeoff: converted.takeoff.clone(),
                                    landing: converted.landing.clone(),
                                    duration_secs,
                                    aircraft,
                                });
                            }
                            Err(e) => {
                                if let Ok(mut state) = progress.lock() {
                                    state.errors.push(ProgressError {
                                        file: name.clone(),
                                        message: e.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut state) = progress.lock() {
                            state.errors.push(ProgressError {
                                file: name.clone(),
                                message: e.to_string(),
                            });
                        }
                    }
                }

                if let Ok(mut state) = progress.lock() {
                    state.completed += 1;
                }
            }

            // Persistent pilot log: append this run's rows to whatever's
            // already at {output}/Pilot Logs/Flight Log.xlsx (if anything),
            // so a workbook builds up across every import run rather than
            // only ever reflecting the most recent one.
            if !pilot_rows.is_empty() {
                let pilot_log_dir = output.join("Pilot Logs");
                let pilot_log_path = pilot_log_dir.join("Flight Log.xlsx");
                let existing = std::fs::read(&pilot_log_path).ok();

                match dji2kmz_core::pilotlog::update_pilot_log(existing.as_deref(), &pilot_rows) {
                    Ok(bytes) => {
                        if let Err(e) = std::fs::create_dir_all(&pilot_log_dir)
                            .and_then(|_| std::fs::write(&pilot_log_path, bytes))
                        {
                            if let Ok(mut state) = progress.lock() {
                                state.errors.push(ProgressError {
                                    file: "Pilot Log".to_string(),
                                    message: format!("Failed to write Flight Log.xlsx: {e}"),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut state) = progress.lock() {
                            state.errors.push(ProgressError {
                                file: "Pilot Log".to_string(),
                                message: format!("Failed to update pilot log: {e}"),
                            });
                        }
                    }
                }
            }

            if let Ok(mut state) = progress.lock() {
                state.done = true;
                state.running = false;
                state.current_file = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Recursively collects every `.kmz` file under `dir` — output now
    /// lives nested under `KMZs/{project}/{date}/`, not flat.
    fn find_kmz_files(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return found;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                found.extend(find_kmz_files(&path));
            } else if path.extension().and_then(|x| x.to_str()) == Some("kmz") {
                found.push(path);
            }
        }
        found
    }

    fn wait_until_done(app: &DjiKmzApp) -> ProgressState {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let snapshot = app.progress.lock().unwrap().clone();
            if snapshot.done {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "conversion did not finish in time");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Exercises the real batch-conversion path (folder listing, background
    /// thread, shared progress state) without needing an actual window —
    /// the eframe::App::ui() rendering itself can't run headless, but all
    /// the logic it drives is plain code, testable directly.
    #[test]
    fn batch_converts_a_real_folder_and_reports_progress() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }

        let input_dir = std::env::temp_dir().join("dji2kmz_app_test_input");
        let output_dir = std::env::temp_dir().join("dji2kmz_app_test_output");
        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
        std::fs::create_dir_all(&input_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();

        // A non-.txt file in the same folder must be ignored.
        std::fs::write(input_dir.join("notes.pdf"), b"not a log").unwrap();
        std::fs::copy(&fixture, input_dir.join("sample.txt")).unwrap();

        let app = DjiKmzApp {
            input_folder: Some(input_dir.clone()),
            output_folder: Some(output_dir.clone()),
            ..Default::default()
        };
        app.start_conversion();

        let snapshot = wait_until_done(&app);
        assert_eq!(snapshot.total, 1, "only the .txt file should be counted, not the .pdf");
        assert_eq!(snapshot.completed, 1);
        assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        // The individual file (new date/time/folder-name format) should
        // land somewhere under output_dir/KMZs/{project}/{date}/.
        let kmzs_root = output_dir.join("KMZs");
        assert!(kmzs_root.is_dir(), "expected a KMZs/ folder under the destination");
        let kmz_files = find_kmz_files(&kmzs_root);
        assert_eq!(kmz_files.len(), 1, "expected exactly one individual .kmz, found: {kmz_files:?}");

        // Pilot log spreadsheet should also have been written.
        let pilot_log = output_dir.join("Pilot Logs").join("Flight Log.xlsx");
        assert!(pilot_log.exists(), "expected Pilot Logs/Flight Log.xlsx to be created");

        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
    }

    /// Two copies of the same real log land on the identical computed
    /// output name (same embedded date/time, same folder) — this exercises
    /// the collision-dedup suffix against real data.
    #[test]
    fn dedupes_identical_filenames() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }

        let input_dir = std::env::temp_dir().join("dji2kmz_dedupe_test_input");
        let output_dir = std::env::temp_dir().join("dji2kmz_dedupe_test_output");
        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
        std::fs::create_dir_all(&input_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();

        // Same content, and both filenames embed the identical
        // "[08-18-13]" bracket time (as if the OS appended " (1)" to a
        // duplicate download) — both should extract the same local
        // date/time and therefore compute the same base output name.
        std::fs::copy(&fixture, input_dir.join("DJIFlightRecord_2026-06-15_[08-18-13].txt")).unwrap();
        std::fs::copy(&fixture, input_dir.join("DJIFlightRecord_2026-06-15_[08-18-13] (1).txt")).unwrap();

        let app = DjiKmzApp {
            input_folder: Some(input_dir.clone()),
            output_folder: Some(output_dir.clone()),
            ..Default::default()
        };
        app.start_conversion();

        let snapshot = wait_until_done(&app);
        assert_eq!(snapshot.completed, 2);
        assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let kmz_files = find_kmz_files(&output_dir.join("KMZs"));
        // 2 individual files, one deduped with " (2)".
        assert_eq!(kmz_files.len(), 2, "found: {kmz_files:?}");
        assert!(
            kmz_files.iter().any(|p| p.file_name().unwrap().to_string_lossy().contains("(2)")),
            "expected a collision-deduped filename among: {kmz_files:?}"
        );

        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
    }

    /// A file placed in a pilot subfolder ({input}/{Pilot}/*.txt) must
    /// still resolve LOCATION from the top-level selected folder (not the
    /// pilot subfolder it directly sits in — the exact regression risk
    /// introduced by making the scan recursive), and the resulting KMZ's
    /// description must carry the pilot's name.
    #[test]
    fn extracts_pilot_from_subfolder_and_keeps_location_from_the_root() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }

        let input_dir = std::env::temp_dir().join("dji2kmz_pilot_test_input");
        let output_dir = std::env::temp_dir().join("dji2kmz_pilot_test_output");
        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
        let pilot_dir = input_dir.join("Jane_Doe");
        std::fs::create_dir_all(&pilot_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::copy(&fixture, pilot_dir.join("sample.txt")).unwrap();

        let app = DjiKmzApp {
            input_folder: Some(input_dir.clone()),
            output_folder: Some(output_dir.clone()),
            ..Default::default()
        };
        app.start_conversion();

        let snapshot = wait_until_done(&app);
        assert_eq!(snapshot.completed, 1);
        assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let kmz_files = find_kmz_files(&output_dir.join("KMZs"));
        assert_eq!(kmz_files.len(), 1, "found: {kmz_files:?}");
        let individual = &kmz_files[0];

        // Location must come from "dji2kmz_pilot_test_input" (the
        // selected root), NOT "Jane_Doe" (the pilot subfolder).
        let individual_name = individual.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            individual_name.contains("dji2kmz_pilot_test_input"),
            "location should come from the root folder, not the pilot subfolder: {individual_name}"
        );
        assert!(
            !individual_name.contains("Jane_Doe"),
            "pilot subfolder name should not leak into the location naming: {individual_name}"
        );

        let kml_bytes = std::fs::read(individual).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(kml_bytes)).unwrap();
        let mut kml = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("doc.kml").unwrap(), &mut kml).unwrap();
        assert!(kml.contains("Pilot: Jane_Doe"), "description should carry the pilot's name: {kml}");

        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
    }

    /// Running conversion twice against the same destination must APPEND
    /// to the pilot log rather than overwrite it — the crux of the
    /// persistence requirement (byte-level correctness of the append is
    /// already covered by dji2kmz_core::pilotlog's own tests; this just
    /// confirms the native app actually reads the existing file back in
    /// before writing, instead of always starting fresh).
    #[test]
    fn pilot_log_persists_across_two_separate_conversion_runs() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample.txt");
        if !fixture.exists() {
            eprintln!("skipping: tests/fixtures/sample.txt not present");
            return;
        }

        let input_dir = std::env::temp_dir().join("dji2kmz_pilotlog_persist_test_input");
        let output_dir = std::env::temp_dir().join("dji2kmz_pilotlog_persist_test_output");
        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
        std::fs::create_dir_all(&input_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        std::fs::copy(&fixture, input_dir.join("sample.txt")).unwrap();

        let pilot_log_path = output_dir.join("Pilot Logs").join("Flight Log.xlsx");

        let app = DjiKmzApp {
            input_folder: Some(input_dir.clone()),
            output_folder: Some(output_dir.clone()),
            ..Default::default()
        };
        app.start_conversion();
        wait_until_done(&app);
        assert!(pilot_log_path.exists());
        let first_run_bytes = std::fs::read(&pilot_log_path).unwrap();

        // Second run against the same destination.
        let app2 = DjiKmzApp {
            input_folder: Some(input_dir.clone()),
            output_folder: Some(output_dir.clone()),
            ..Default::default()
        };
        app2.start_conversion();
        let snapshot = wait_until_done(&app2);
        assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let second_run_bytes = std::fs::read(&pilot_log_path).unwrap();
        assert!(
            second_run_bytes.len() > first_run_bytes.len(),
            "a second run appending another row should grow the workbook, not just rewrite the same content"
        );

        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
    }
}

fn open_in_file_explorer(path: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

impl eframe::App for DjiKmzApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let snapshot = self
            .progress
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();

        ui.heading("DJI2KMZ");
        ui.label("Batch-convert DJI flight logs (.txt) into flight-path KMZ files.");
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.button("Choose Input Folder...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select folder containing DJI .txt logs")
                    .pick_folder()
                {
                    self.input_folder = Some(path);
                }
            }
            ui.label(
                self.input_folder
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No folder selected".to_string()),
            );
        });

        ui.horizontal(|ui| {
            if ui.button("Choose Destination Folder...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select the destination folder (KMZs/ and Pilot Logs/ will be created inside it)")
                    .pick_folder()
                {
                    self.output_folder = Some(path);
                }
            }
            ui.label(
                self.output_folder
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No folder selected".to_string()),
            );
        });
        ui.label(
            egui::RichText::new(
                "A KMZs/{project}/{date}/ folder and a Pilot Logs/Flight Log.xlsx spreadsheet are created inside whatever folder you choose here.",
            )
            .color(egui::Color32::GRAY)
            .small(),
        );

        ui.add_space(12.0);

        let can_convert =
            self.input_folder.is_some() && self.output_folder.is_some() && !snapshot.running;
        if ui
            .add_enabled(can_convert, egui::Button::new("Convert"))
            .clicked()
        {
            self.start_conversion();
        }

        ui.add_space(12.0);

        if snapshot.running {
            let current = snapshot
                .current_file
                .as_ref()
                .map(|f| format!(" — {f}"))
                .unwrap_or_default();
            ui.label(format!(
                "Converting {} of {}{}",
                snapshot.completed, snapshot.total, current
            ));
            ui.ctx().request_repaint();
        }

        if snapshot.done {
            let error_count = snapshot.errors.len();
            let success_count = snapshot.completed.saturating_sub(error_count);
            ui.label(format!(
                "Converted {} of {} files.{}",
                success_count,
                snapshot.total,
                if error_count > 0 {
                    format!(" {error_count} error(s).")
                } else {
                    String::new()
                }
            ));

            if error_count > 0 {
                ui.collapsing("Show errors", |ui| {
                    for err in &snapshot.errors {
                        ui.label(format!("{}: {}", err.file, err.message));
                    }
                });
            }

            ui.add_space(8.0);
            if let Some(dir) = &snapshot.output_dir {
                ui.horizontal(|ui| {
                    if ui.button("Open Output Folder").clicked() {
                        open_in_file_explorer(dir);
                    }
                    let mut dir_text = dir.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut dir_text)
                            .desired_width(300.0)
                            .interactive(false),
                    );
                    if ui.button("Copy Path").clicked() {
                        ui.ctx().copy_text(dir.clone());
                    }
                });
            }
        }
    }
}
