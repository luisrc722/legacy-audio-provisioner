use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

/// [R-01-007] Abstraccion de Progreso
/// Precondicion: el flujo de provisionamiento reporta eventos de avance desacoplados de la salida concreta.
/// Postcondicion: el orquestador puede ejecutar progreso/info en modo CLI o JSON sin acoplarse a una UI fija.
/// Invariante: la logica de negocio nunca depende de `println!` directo para reportar avance.
pub trait ProgressReporter {
    fn info(&self, msg: &str);
    fn start_progress(&mut self, total: u64) -> Result<()>;
    fn inc_progress(&mut self, step: u64, msg: &str);
    fn finish(&mut self, msg: &str);
}

pub struct CliReporter {
    progress: Option<ProgressBar>,
}

impl CliReporter {
    pub fn new() -> Self {
        Self { progress: None }
    }
}

impl ProgressReporter for CliReporter {
    fn info(&self, msg: &str) {
        println!("{}", msg);
    }

    fn start_progress(&mut self, total: u64) -> Result<()> {
        let pb = ProgressBar::new(total);
        let style = ProgressStyle::with_template(
            "{spinner} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})\n{msg}",
        )?
        .progress_chars("#>-")
        .tick_chars("-\\|/");
        pb.set_style(style);
        self.progress = Some(pb);
        Ok(())
    }

    fn inc_progress(&mut self, step: u64, msg: &str) {
        if let Some(pb) = &self.progress {
            pb.set_message(msg.to_string());
            pb.inc(step);
        }
    }

    fn finish(&mut self, msg: &str) {
        if let Some(pb) = &self.progress {
            pb.finish_with_message(msg.to_string());
        }
        self.progress = None;
    }
}

pub struct JsonIpcReporter;

impl JsonIpcReporter {
    pub fn new() -> Self {
        Self
    }
}

impl ProgressReporter for JsonIpcReporter {
    fn info(&self, _msg: &str) {}

    fn start_progress(&mut self, _total: u64) -> Result<()> {
        Ok(())
    }

    fn inc_progress(&mut self, _step: u64, _msg: &str) {}

    fn finish(&mut self, _msg: &str) {}
}

pub fn create_reporter(json_mode: bool) -> Box<dyn ProgressReporter> {
    if json_mode {
        Box::new(JsonIpcReporter::new())
    } else {
        Box::new(CliReporter::new())
    }
}
