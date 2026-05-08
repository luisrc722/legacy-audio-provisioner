use anyhow::Result;
use clap::{Parser, Subcommand};
use lap_core::sanitizer;

#[derive(Parser, Debug)]
#[command(name = "lap-cli-tools", version, about = "Herramientas auxiliares LAP")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Sanitize {
        #[arg(long)]
        input: String,
        #[arg(long, default_value_t = 1)]
        index: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Sanitize { input, index } => {
            let clean = sanitizer::sanitize_filename(&input);
            let indexed = sanitizer::add_sequential_prefix(&clean, index);
            println!("{}", indexed);
        }
    }
    Ok(())
}
