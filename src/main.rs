mod cli;
mod far_fmt;
mod gui;

use clap::Parser;
use cli::{Cli, Commands};
use far_fmt::{FarArchive, FileSource};
use std::process;

use gtk4 as gtk;
use gtk::prelude::*;
use gio::ApplicationFlags;

fn main() -> glib::ExitCode {
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
        return glib::ExitCode::SUCCESS;
    }

    // Route 2: GTK Desktop Application
    let app = gtk::Application::builder()
        .application_id("com.az.farxplus")
        .flags(ApplicationFlags::empty())
        .build();

    app.connect_activate(gui::build_ui);
    
    // Run the app (passing empty args since CLI was already parsed)
    app.run_with_args(&Vec::<String>::new())
}