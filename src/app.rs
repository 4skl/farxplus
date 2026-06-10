use eframe::egui;
use rfd::FileDialog;
use std::collections::HashSet;
use std::path::PathBuf;
use crate::far_fmt::{FarArchive, FileSource};

#[derive(Default)]
pub struct FarxPlusApp {
    archives: Vec<FarArchive>,
    
    selected_archives: HashSet<usize>,
    active_archive_idx: Option<usize>, 
    
    selected_items: HashSet<String>,
    last_clicked_item_idx: Option<usize>,
    
    status_message: String,
    
    renaming_archive: Option<(usize, String, bool)>, 
    renaming_item: Option<(String, String, bool)>,
}

impl FarxPlusApp {
    fn open_files_dialog(&mut self) {
        if let Some(paths) = FileDialog::new().add_filter("FAR Archive", &["far"]).pick_files() {
            for path in paths {
                if let Some(idx) = self.archives.iter().position(|a| a.file_path.as_ref() == Some(&path)) {
                    self.active_archive_idx = Some(idx);
                    self.selected_archives.insert(idx);
                    continue;
                }
                match FarArchive::open(&path) {
                    Ok(archive) => {
                        self.archives.push(archive);
                        let new_idx = self.archives.len() - 1;
                        self.active_archive_idx = Some(new_idx);
                        self.selected_archives.clear();
                        self.selected_archives.insert(new_idx);
                        self.status_message = "Archive loaded.".to_string();
                    }
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
        }
    }

    fn save_as_dialog(&mut self) {
        if let Some(idx) = self.active_archive_idx {
            let archive = &mut self.archives[idx];
            
            // Generate a smart default name based on the tab's current display name
            let default_name = if archive.display_name.to_lowercase().ends_with(".far") {
                archive.display_name.clone()
            } else {
                format!("{}.far", archive.display_name)
            };

            if let Some(mut out_path) = FileDialog::new()
                .set_title("Save FAR Archive As")
                .add_filter("FAR Archive", &["far"])
                .set_file_name(&default_name)
                .save_file() 
            {
                // Force extension safety check
                if out_path.extension().unwrap_or_default() != "far" {
                    out_path.set_extension("far");
                }

                self.status_message = "Saving...".to_string();
                match archive.save_to_disk(&out_path) {
                    Ok(_) => {
                        archive.is_modified = false;
                        archive.file_path = Some(out_path.clone());
                        archive.display_name = out_path.file_name().unwrap().to_string_lossy().to_string();
                        self.status_message = "Saved successfully!".to_string();
                    }
                    Err(e) => self.status_message = format!("Save failed: {}", e),
                }
            }
        }
    }

    fn walk_and_add_dropped(&mut self, idx: usize, disk_path: PathBuf, virtual_parent: &str) {
        if disk_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&disk_path) {
                let folder_name = disk_path.file_name().unwrap().to_string_lossy();
                let new_parent = if virtual_parent.is_empty() { folder_name.to_string() } else { format!("{}/{}", virtual_parent, folder_name) };
                for entry in entries.flatten() {
                    self.walk_and_add_dropped(idx, entry.path(), &new_parent);
                }
            }
        } else {
            let filename = disk_path.file_name().unwrap().to_string_lossy();
            let v_path = if virtual_parent.is_empty() { filename.to_string() } else { format!("{}/{}", virtual_parent, filename) };
            FarArchive::insert_node(&mut self.archives[idx].tree_root, &v_path, FileSource::OnDisk(disk_path));
            self.archives[idx].is_modified = true;
        }
    }
}

impl eframe::App for FarxPlusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut request_repaint = false;

        // --- 1. Global Drag & Drop Handling ---
        // By pulling from `ui.ctx()` directly, we bypass any panels that might be eating the drop events.
        let dropped_files = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() {
            if let Some(idx) = self.active_archive_idx {
                for file in dropped_files {
                    if let Some(path) = file.path {
                        self.walk_and_add_dropped(idx, path, "");
                    }
                }
                self.status_message = "Added dropped files to archive.".to_string();
            } else {
                self.status_message = "Please open or create an archive before dropping files.".to_string();
            }
        }

        // --- 2. Top Menu ---
        egui::Panel::top("top").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Archive").clicked() {
                        self.archives.push(FarArchive::new_empty("New_Archive.far".to_string()));
                        let new_idx = self.archives.len() - 1;
                        self.active_archive_idx = Some(new_idx);
                        self.selected_archives.clear();
                        self.selected_archives.insert(new_idx);
                        ui.close();
                    }
                    if ui.button("Open...").clicked() {
                        self.open_files_dialog();
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(self.active_archive_idx.is_some(), egui::Button::new("Save As...")).clicked() {
                        self.save_as_dialog();
                        ui.close();
                    }
                });
            });
        });

        // --- 3. Status Bar ---
        egui::Panel::bottom("status").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.strong("Status:");
                ui.label(&self.status_message);
            });
            ui.add_space(4.0);
        });

        // --- 4. Left Sidebar (Workspace / Archives) ---
        egui::Panel::left("workspace").resizable(true).show_inside(ui, |ui| {
            ui.heading("Workspace");
            ui.separator();
            
            let mut close_indices = Vec::new();
            
            for (i, archive) in self.archives.iter_mut().enumerate() {
                let is_selected = self.selected_archives.contains(&i);
                let display = if archive.is_modified { format!("* {}", archive.display_name) } else { archive.display_name.clone() };
                
                if let Some((renaming_idx, mut buffer, request_focus)) = self.renaming_archive.take() {
                    if renaming_idx == i {
                        let response = ui.text_edit_singleline(&mut buffer);
                        if request_focus { response.request_focus(); }
                        
                        if response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if !buffer.trim().is_empty() { archive.display_name = buffer; }
                            self.renaming_archive = None;
                        } else {
                            self.renaming_archive = Some((i, buffer, false));
                        }
                        continue; 
                    } else {
                        self.renaming_archive = Some((renaming_idx, buffer, request_focus));
                    }
                }

                let response = ui.selectable_label(is_selected, display);
                
                if response.clicked() {
                    if ui.input(|i| i.modifiers.ctrl) {
                        if is_selected { self.selected_archives.remove(&i); } 
                        else { self.selected_archives.insert(i); }
                        self.active_archive_idx = Some(i);
                    } else {
                        self.selected_archives.clear();
                        self.selected_archives.insert(i);
                        self.active_archive_idx = Some(i);
                        self.selected_items.clear();
                    }
                }

                if response.double_clicked() {
                    self.renaming_archive = Some((i, archive.display_name.clone(), true));
                }

                response.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        self.renaming_archive = Some((i, archive.display_name.clone(), true));
                        ui.close();
                    }
                    if ui.button("Close Archive").clicked() {
                        close_indices.push(i);
                        ui.close();
                    }
                });
            }

            close_indices.sort_unstable_by(|a, b| b.cmp(a));
            for idx in close_indices {
                self.archives.remove(idx);
                self.selected_archives.remove(&idx);
                if self.active_archive_idx == Some(idx) { self.active_archive_idx = None; }
            }
        });

        // --- 5. Main Central Panel (Virtual Tree Items) ---
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(idx) = self.active_archive_idx {
                let archive = &mut self.archives[idx];
                ui.heading(&archive.display_name);
                ui.horizontal(|ui| {
                    ui.strong("Name");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.strong("Size"); });
                });
                ui.separator();

                let flat_tree = archive.flatten_tree();
                let row_height = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
                
                let mut path_to_toggle = None;
                let mut paths_to_delete = Vec::new();

                egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_height, flat_tree.len(), |ui, row_range| {
                    for i in row_range {
                        let node = &flat_tree[i];
                        let is_selected = self.selected_items.contains(&node.path);

                        ui.horizontal(|ui| {
                            ui.add_space(node.depth as f32 * 15.0);
                            
                            if let Some((renaming_path, mut buffer, request_focus)) = self.renaming_item.take() {
                                if renaming_path == node.path {
                                    let response = ui.text_edit_singleline(&mut buffer);
                                    if request_focus { response.request_focus(); }
                                    
                                    if response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        if !buffer.trim().is_empty() {
                                            archive.rename_node(&node.path, &buffer);
                                            self.selected_items.clear(); 
                                        }
                                        self.renaming_item = None;
                                    } else {
                                        self.renaming_item = Some((renaming_path, buffer, false));
                                    }
                                    return; 
                                } else {
                                    self.renaming_item = Some((renaming_path, buffer, request_focus));
                                }
                            }

                            let icon = if node.is_dir { if node.expanded { "📂" } else { "📁" } } else { "📄" };
                            let label = format!("{} {}", icon, node.name);
                            
                            let response = ui.selectable_label(is_selected, label);
                            
                            if response.clicked() {
                                let ctrl = ui.input(|i| i.modifiers.ctrl);
                                let shift = ui.input(|i| i.modifiers.shift);

                                if shift && self.last_clicked_item_idx.is_some() {
                                    let start = self.last_clicked_item_idx.unwrap().min(i);
                                    let end = self.last_clicked_item_idx.unwrap().max(i);
                                    if !ctrl { self.selected_items.clear(); }
                                    for j in start..=end {
                                        self.selected_items.insert(flat_tree[j].path.clone());
                                    }
                                } else if ctrl {
                                    if is_selected { self.selected_items.remove(&node.path); } 
                                    else { self.selected_items.insert(node.path.clone()); }
                                    self.last_clicked_item_idx = Some(i);
                                } else {
                                    if node.is_dir { path_to_toggle = Some(node.path.clone()); }
                                    self.selected_items.clear();
                                    self.selected_items.insert(node.path.clone());
                                    self.last_clicked_item_idx = Some(i);
                                }
                            }

                            if response.double_clicked() {
                                self.renaming_item = Some((node.path.clone(), node.name.clone(), true));
                            }

                            response.context_menu(|ui| {
                                if !self.selected_items.contains(&node.path) {
                                    self.selected_items.clear();
                                    self.selected_items.insert(node.path.clone());
                                }
                                
                                let selection_count = self.selected_items.len();
                                ui.label(egui::RichText::new(format!("{} items selected", selection_count)).weak());
                                ui.separator();

                                if selection_count == 1 {
                                    if ui.button("Rename").clicked() {
                                        self.renaming_item = Some((node.path.clone(), node.name.clone(), true));
                                        ui.close();
                                    }
                                }
                                
                                if ui.button("Extract Selected...").clicked() {
                                    if let Some(out_dir) = FileDialog::new().set_title("Extract To...").pick_folder() {
                                        self.status_message = "Extracting selected items...".to_string();
                                        if let Err(e) = archive.extract_items(&self.selected_items, &out_dir) {
                                            self.status_message = format!("Extraction failed: {}", e);
                                        } else {
                                            self.status_message = "Extraction complete!".to_string();
                                        }
                                    }
                                    ui.close();
                                }

                                if ui.button("Delete").clicked() {
                                    paths_to_delete = self.selected_items.iter().cloned().collect();
                                    ui.close();
                                }
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !node.is_dir { ui.label(format!("{} B", node.size)); }
                            });
                        });
                    }
                });

                if let Some(p) = path_to_toggle { archive.toggle_expansion(&p); }
                for p in paths_to_delete { 
                    archive.remove_node(&p); 
                    self.selected_items.remove(&p);
                }

            } else {
                ui.centered_and_justified(|ui| { ui.label("Drag & Drop files here, or open an archive."); });
            }
        });

        if request_repaint { ui.ctx().request_repaint(); }
    }
}