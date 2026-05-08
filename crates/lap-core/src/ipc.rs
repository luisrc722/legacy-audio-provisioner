use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "event", content = "payload")]
pub enum IpcEvent {
    #[serde(rename = "PROGRESS")]
    Progress {
        files_processed: usize,
        total_files: usize,
        percentage: f64,
        current_file: String,
        eta_seconds: u64,
    },
    #[serde(rename = "WARNING")]
    Warning {
        code: String,
        source_file: String,
        message: String,
    },
    #[serde(rename = "FATAL_ERROR")]
    FatalError {
        code: String,
        message: String,
        action_required: String,
    },
    #[serde(rename = "SUCCESS")]
    Success {
        total_processed: usize,
        total_skipped: usize,
        elapsed_time_seconds: u64,
        message: String,
    },
}

impl IpcEvent {
    pub fn emit(&self, json_mode: bool) {
        if json_mode {
            if let Ok(line) = serde_json::to_string(self) {
                println!("{}", line);
            }
        }
    }
}
