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

        let mut files: Vec<PathBuf> = std::fs::read_dir(&input)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .map(|ext| ext.eq_ignore_ascii_case("txt"))
                        .unwrap_or(false)
            })
            .collect();
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
            for file in files {
                let name = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();

                if let Ok(mut state) = progress.lock() {
                    state.current_file = Some(name.clone());
                }

                if let Err(e) = crate::dji::convert_file(&file, &output, &api_key) {
                    if let Ok(mut state) = progress.lock() {
                        state.errors.push(ProgressError {
                            file: name.clone(),
                            message: e.to_string(),
                        });
                    }
                }

                if let Ok(mut state) = progress.lock() {
                    state.completed += 1;
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
                assert!(output_dir.join("sample.kmz").exists());
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
