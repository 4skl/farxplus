// src/main.rs
mod cli;
mod app;
mod far_fmt;

use clap::Parser;
use cli::{Cli, Commands};
use app::FarxPlusApp;

fn main() -> eframe::Result<()> {
    let cli_args = Cli::parse();

    // Route 1: Command Line Mode
    if let Some(command) = cli_args.command {
        match command {
            Commands::Extract { input, output } => {
                println!("CLI: Extracting {:?} to {:?}", input, output);
                // Call far_fmt extraction logic here
            }
            Commands::Pack { input_dir, output } => {
                println!("CLI: Packing {:?} into {:?}", input_dir, output);
                // Call far_fmt packing logic here
            }
        }
        return Ok(()); // Exit after CLI tasks
    }

    // Route 2: Graphical User Interface Mode
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("FARxPlus"),
        ..Default::default()
    };

    eframe::run_native(
        "FARxPlus",
        options,
        Box::new(|_cc| Ok(Box::new(FarxPlusApp::default()))),
    )
}