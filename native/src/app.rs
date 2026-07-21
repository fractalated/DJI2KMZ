use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
            let mut flights = Vec::new();
            let mut dates = Vec::new();
            // Same name as the individual .kmz file (minus extension, but
            // including any " (2)" collision-dedup suffix), so a flight's
            // layer in the merged KMZ is identifiable as the same flight —
            // meta.display_name alone is often identical across every
            // flight from the same aircraft.
            let mut names = Vec::new();

            for file in files {
                let name = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();

                if let Ok(mut state) = progress.lock() {
                    state.current_file = Some(name.clone());
                }

                match crate::dji::convert_file(&file, &input, &output, &api_key) {
                    Ok(outcome) => {
                        let placemark_name = outcome
                            .output_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Flight")
                            .to_string();
                        names.push(placemark_name);
                        dates.push(outcome.local_date);
                        flights.push(outcome.flight_data);
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

            // Merged multi-flight KMZ, produced alongside the individual
            // per-flight files (not instead of them) whenever at least one
            // flight converted successfully.
            if !flights.is_empty() {
                let folder_name = input
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Flight_Logs");
                let title = dji2kmz_core::naming::merged_title(folder_name, &dates);
                let merged_kml = dji2kmz_core::kml::build_merged_kml(&title, &names, &flights);
                let merged_path = output.join(&title).with_extension("kmz");
                match std::fs::File::create(&merged_path) {
                    Ok(file) => {
                        if let Err(e) = dji2kmz_core::kml::write_kmz(file, &merged_kml) {
                            if let Ok(mut state) = progress.lock() {
                                state.errors.push(ProgressError {
                                    file: title.clone(),
                                    message: format!("Failed to write merged KMZ: {e}"),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut state) = progress.lock() {
                            state.errors.push(ProgressError {
                                file: title.clone(),
                                message: format!("Failed to create merged KMZ: {e}"),
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
    use std::time::{Duration, Instant};

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

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let snapshot = app.progress.lock().unwrap().clone();
            if snapshot.done {
                assert_eq!(snapshot.total, 1, "only the .txt file should be counted, not the .pdf");
                assert_eq!(snapshot.completed, 1);
                assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

                // Individual file (new date/time/folder-name format) +
                // merged file should both land in the output folder.
                let kmz_files: Vec<_> = std::fs::read_dir(&output_dir)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("kmz"))
                    .collect();
                assert_eq!(kmz_files.len(), 2, "expected one individual + one merged .kmz, found: {:?}", kmz_files.iter().map(|e| e.file_name()).collect::<Vec<_>>());

                let merged = kmz_files.iter().find(|e| {
                    e.file_name().to_string_lossy().contains("Flight_Logs")
                });
                assert!(merged.is_some(), "expected a merged file with 'Flight_Logs' in its name");
                break;
            }
            assert!(Instant::now() < deadline, "conversion did not finish in time");
            std::thread::sleep(Duration::from_millis(100));
        }

        let _ = std::fs::remove_dir_all(&input_dir);
        let _ = std::fs::remove_dir_all(&output_dir);
    }

    /// Two copies of the same real log land on the identical computed
    /// output name (same embedded date/time, same folder) — this exercises
    /// the collision-dedup suffix against real data, and confirms the
    /// merged KMZ ends up with both flights as separate placemarks even
    /// though they're same-day (single-date title, not a range).
    #[test]
    fn dedupes_identical_filenames_and_merges_both_flights() {
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

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let snapshot = app.progress.lock().unwrap().clone();
            if snapshot.done {
                assert_eq!(snapshot.completed, 2);
                assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

                let kmz_files: Vec<String> = std::fs::read_dir(&output_dir)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("kmz"))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();

                // 2 individual (one deduped with " (2)") + 1 merged = 3 files.
                assert_eq!(kmz_files.len(), 3, "found: {kmz_files:?}");
                assert!(
                    kmz_files.iter().any(|n| n.contains("(2)")),
                    "expected a collision-deduped filename among: {kmz_files:?}"
                );

                let merged_name = kmz_files.iter().find(|n| n.contains("Flight_Logs"))
                    .unwrap_or_else(|| panic!("expected a merged file among: {kmz_files:?}"));
                // Both flights are the same day, so the title should carry
                // a single date, not a "--" range.
                assert!(!merged_name.contains("--"), "same-day merge shouldn't produce a date range: {merged_name}");

                let merged_bytes = std::fs::read(output_dir.join(merged_name)).unwrap();
                let mut archive = zip::ZipArchive::new(std::io::Cursor::new(merged_bytes)).unwrap();
                let mut kml = String::new();
                std::io::Read::read_to_string(&mut archive.by_name("doc.kml").unwrap(), &mut kml).unwrap();
                let placemark_count = kml.matches("<Placemark>").count();
                assert_eq!(placemark_count, 2, "merged KMZ should contain both flights as separate placemarks");

                break;
            }
            assert!(Instant::now() < deadline, "conversion did not finish in time");
            std::thread::sleep(Duration::from_millis(100));
        }

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

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let snapshot = app.progress.lock().unwrap().clone();
            if snapshot.done {
                assert_eq!(snapshot.completed, 1);
                assert!(snapshot.errors.is_empty(), "errors: {:?}", snapshot.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

                let kmz_files: Vec<_> = std::fs::read_dir(&output_dir)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("kmz"))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();

                // Location must come from "dji2kmz_pilot_test_input" (the
                // selected root), NOT "Jane_Doe" (the pilot subfolder).
                let individual = kmz_files.iter().find(|n| !n.contains("Flight_Logs"))
                    .unwrap_or_else(|| panic!("expected an individual file among: {kmz_files:?}"));
                assert!(
                    individual.contains("dji2kmz_pilot_test_input"),
                    "location should come from the root folder, not the pilot subfolder: {individual}"
                );
                assert!(
                    !individual.contains("Jane_Doe"),
                    "pilot subfolder name should not leak into the location naming: {individual}"
                );

                let kml_bytes = std::fs::read(output_dir.join(individual)).unwrap();
                let mut archive = zip::ZipArchive::new(std::io::Cursor::new(kml_bytes)).unwrap();
                let mut kml = String::new();
                std::io::Read::read_to_string(&mut archive.by_name("doc.kml").unwrap(), &mut kml).unwrap();
                assert!(kml.contains("Pilot: Jane_Doe"), "description should carry the pilot's name: {kml}");

                break;
            }
            assert!(Instant::now() < deadline, "conversion did not finish in time");
            std::thread::sleep(Duration::from_millis(100));
        }

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
            if ui.button("Choose Output Folder...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select folder to save .kmz files into")
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
