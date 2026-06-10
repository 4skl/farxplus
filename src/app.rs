use eframe::egui;
use rfd::FileDialog;
use crate::far_fmt::FarArchive;

#[derive(Default)]
pub struct FarxPlusApp {
    current_archive: Option<FarArchive>,
    status_message: String,
}

impl FarxPlusApp {
    fn open_file_dialog(&mut self) {
        if let Some(path) = FileDialog::new()
            .set_title("Open FAR Archive")
            .add_filter("FAR Archive", &["far"])
            .pick_file() 
        {
            match FarArchive::open(&path) {
                Ok(archive) => {
                    self.status_message = format!("Loaded: {:?}", path.file_name().unwrap_or_default());
                    self.current_archive = Some(archive);
                }
                Err(e) => {
                    self.status_message = format!("Error: {}", e);
                }
            }
        }
    }

    fn extract_current_archive(&mut self) {
        if let Some(archive) = &self.current_archive {
            if let Some(output_dir) = FileDialog::new()
                .set_title("Select Output Directory")
                .pick_folder() 
            {
                self.status_message = "Extracting...".to_string();
                match archive.extract_all(&output_dir) {
                    Ok(_) => {
                        self.status_message = "Extraction complete!".to_string();
                    }
                    Err(e) => {
                        self.status_message = format!("Extraction failed: {}", e);
                    }
                }
            }
        }
    }
}

impl eframe::App for FarxPlusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        
        // 1. Keyboard Shortcuts
        if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O))) {
            self.open_file_dialog();
        }

        // 2. Top Menu Bar (Docked to Top)
        egui::TopBottomPanel::top("top_panel").show_inside(ui, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Archive... (Ctrl+O)").clicked() {
                        self.open_file_dialog();
                        ui.close_menu();
                    }
                    
                    if ui.add_enabled(self.current_archive.is_some(), egui::Button::new("Extract All")).clicked() {
                        self.extract_current_archive();
                        ui.close_menu();
                    }
                    
                    ui.separator();
                    
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About FARxPlus").clicked() {
                        self.status_message = "FARxPlus: A modern Sims 1 .far archive manager".to_string();
                        ui.close_menu();
                    }
                });
            });
        });

        // 3. Status Bar (Docked to Bottom)
        egui::TopBottomPanel::bottom("status_panel").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Status:").strong());
                ui.label(&self.status_message);
            });
            ui.add_space(4.0);
        });

        // 4. Main Content Area (Fills remaining space)
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(archive) = &self.current_archive {
                ui.heading(format!(
                    "Archive Contents ({} files)",
                    archive.entries.len()
                ));
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.strong("Filename");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.strong("Size (Bytes)");
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in &archive.entries {
                            ui.horizontal(|ui| {
                                ui.label(&entry.filename);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(entry.original_size.to_string());
                                });
                            });
                        }
                    });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("No archive loaded. Go to File -> Open or press Ctrl+O.");
                });
            }
        });
    }
}