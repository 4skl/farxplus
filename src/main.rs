mod cli;
mod app;
mod far_fmt;

use clap::Parser;
use cli::{Cli, Commands};
use app::FarxPlusApp;
use far_fmt::{FarArchive, FileSource};
use std::process;

fn main() -> eframe::Result<()> {
    let cli_args = Cli::parse();

    // Route 1: Command Line Interface
    if let Some(command) = cli_args.command {
        match command {
            Commands::Extract { input, output } => {
                println!("📦 Extracting: {:?} -> {:?}", input, output);
                match FarArchive::open(&input) {
                    Ok(archive) => {
                        if let Err(e) = archive.extract_all(&output) {
                            eprintln!("❌ Extraction failed: {}", e);
                            process::exit(1);
                        }
                        println!("✅ Extraction complete!");
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to open archive: {}", e);
                        process::exit(1);
                    }
                }
            }
            Commands::Pack { input_dir, output } => {
                println!("🗜️ Packing: {:?} -> {:?}", input_dir, output);
                
                // Construct a virtual tree from the directory, then save it
                let mut archive = FarArchive::new_empty("CLI_Pack.far".to_string());
                
                let mut dirs_to_visit = vec![(input_dir.clone(), String::new())];
                while let Some((dir, virtual_parent)) = dirs_to_visit.pop() {
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let name = path.file_name().unwrap().to_string_lossy();
                            let v_path = if virtual_parent.is_empty() { name.to_string() } else { format!("{}/{}", virtual_parent, name) };
                            
                            if path.is_dir() {
                                dirs_to_visit.push((path, v_path));
                            } else {
                                FarArchive::insert_node(&mut archive.tree_root, &v_path, FileSource::OnDisk(path));
                            }
                        }
                    }
                }

                if let Err(e) = archive.save_to_disk(&output) {
                    eprintln!("❌ Packing failed: {}", e);
                    process::exit(1);
                }
                println!("✅ Packing complete!");
            }
        }
        process::exit(0);
    }

    // Route 2: Desktop Application
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title("FARxPlus"),
        ..Default::default()
    };

    eframe::run_native(
        "FARxPlus",
        options,
        Box::new(|_cc| Ok(Box::new(FarxPlusApp::default()))),
    )
}