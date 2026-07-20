use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct ProgressError {
    pub file: String,
    pub message: String,
}

#[derive(Default, Clone)]
pub struct ProgressState {
    pub total: usize,
    pub completed: usize,
    pub current_file: Option<String>,
    pub done: bool,
    pub running: bool,
    pub errors: Vec<ProgressError>,
    pub output_dir: Option<String>,
}

pub type SharedProgress = Arc<Mutex<ProgressState>>;
