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

    // Volatile memory for the last browsed directory
    last_browsed_dir: Option<PathBuf>,
}

impl FarxPlusApp {
    /// Helper to cleanly load an archive from a path
    fn load_archive(&mut self, path: PathBuf) {
        if let Some(idx) = self.archives.iter().position(|a| a.file_path.as_ref() == Some(&path)) {
            self.active_archive_idx = Some(idx);
            self.selected_archives.clear();
            self.selected_archives.insert(idx);
            return;
        }
        match FarArchive::open(&path) {
            Ok(archive) => {
                let name = archive.display_name.clone();
                self.archives.push(archive);
                let new_idx = self.archives.len() - 1;
                self.active_archive_idx = Some(new_idx);
                self.selected_archives.clear();
                self.selected_archives.insert(new_idx);
                self.status_message = format!("Archive '{}' loaded.", name);
            }
            Err(e) => self.status_message = format!("Error loading archive: {}", e),
        }
    }

    /// Helper to update the last browsed directory based on a selected file/folder
    fn update_last_dir_from_path(&mut self, path: &PathBuf) {
        if path.is_file() {
            if let Some(parent) = path.parent() {
                self.last_browsed_dir = Some(parent.to_path_buf());
            }
        } else if path.is_dir() {
            self.last_browsed_dir = Some(path.clone());
        }
    }

    fn open_files_dialog(&mut self) {
        let mut dialog = FileDialog::new().add_filter("FAR Archive", &["far"]);
        if let Some(dir) = &self.last_browsed_dir {
            dialog = dialog.set_directory(dir);
        }

        if let Some(paths) = dialog.pick_files() {
            if let Some(first_path) = paths.first() {
                self.update_last_dir_from_path(first_path);
            }
            for path in paths {
                self.load_archive(path);
            }
        }
    }

    fn save_as_dialog(&mut self) {
        if let Some(idx) = self.active_archive_idx {
            // Scope the immutable borrow of `self.archives` so it drops immediately
            let default_name = {
                let archive = &self.archives[idx];
                if archive.display_name.to_lowercase().ends_with(".far") {
                    archive.display_name.clone()
                } else {
                    format!("{}.far", archive.display_name)
                }
            };

            let mut dialog = FileDialog::new()
                .set_title("Save FAR Archive As")
                .add_filter("FAR Archive", &["far"])
                .set_file_name(&default_name);
            
            if let Some(dir) = &self.last_browsed_dir {
                dialog = dialog.set_directory(dir);
            }

            if let Some(mut out_path) = dialog.save_file() {
                // `self` is fully available here to mutate the path
                self.update_last_dir_from_path(&out_path);

                if out_path.extension().unwrap_or_default() != "far" {
                    out_path.set_extension("far");
                }

                self.status_message = "Saving...".to_string();
                
                // Re-borrow mutably now that `update_last_dir_from_path` is done
                let archive = &mut self.archives[idx];
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

   fn add_disk_item(&mut self, idx: usize, disk_path: PathBuf, virtual_parent: &str) {
        if disk_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&disk_path) {
                let folder_name = disk_path.file_name().unwrap().to_string_lossy();
                let new_parent = if virtual_parent.is_empty() { 
                    folder_name.to_string() 
                } else { 
                    format!("{}/{}", virtual_parent, folder_name) 
                };
                
                // FIX: Do NOT insert the directory itself as a node. 
                // Let the recursive file insertions build the tree naturally.

                for entry in entries.flatten() {
                    self.add_disk_item(idx, entry.path(), &new_parent);
                }
            }
        } else {
            let filename = disk_path.file_name().unwrap().to_string_lossy();
            let v_path = if virtual_parent.is_empty() { filename.to_string() } else { format!("{}/{}", virtual_parent, filename) };
            FarArchive::insert_node(&mut self.archives[idx].tree_root, &v_path, FileSource::OnDisk(disk_path));
            self.archives[idx].is_modified = true;
        }
    }

    fn manual_add_files_dialog(&mut self, virtual_parent: &str) {
        if let Some(idx) = self.active_archive_idx {
            let mut dialog = FileDialog::new().set_title("Select Files to Add");
            if let Some(dir) = &self.last_browsed_dir {
                dialog = dialog.set_directory(dir);
            }

            if let Some(paths) = dialog.pick_files() {
                if let Some(first_path) = paths.first() {
                    self.update_last_dir_from_path(first_path);
                }
                for path in paths {
                    self.add_disk_item(idx, path, virtual_parent);
                }
                self.status_message = format!("Files added to /{}", virtual_parent);
            }
        }
    }

    fn manual_add_folder_dialog(&mut self, virtual_parent: &str) {
        if let Some(idx) = self.active_archive_idx {
            let mut dialog = FileDialog::new().set_title("Select Folder to Add");
            if let Some(dir) = &self.last_browsed_dir {
                dialog = dialog.set_directory(dir);
            }

            if let Some(path) = dialog.pick_folder() {
                self.update_last_dir_from_path(&path);
                self.add_disk_item(idx, path, virtual_parent);
                self.status_message = format!("Folder added to /{}", virtual_parent);
            }
        }
    }
}

impl eframe::App for FarxPlusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut request_repaint = false;

        let mut active_target_dir = String::new();
        if let Some(idx) = self.active_archive_idx {
            let archive = &self.archives[idx];
            if self.selected_items.len() == 1 {
                let sel = self.selected_items.iter().next().unwrap();
                if let Some(node) = archive.get_node(sel) {
                    if node.is_dir {
                        active_target_dir = sel.clone();
                    } else if let Some(pos) = sel.rfind('/') {
                        active_target_dir = sel[..pos].to_string(); 
                    }
                }
            }
        }

        // --- 1. Top Menu ---
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

        // --- 2. Status Bar ---
        egui::Panel::bottom("status").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.strong("Status:");
                ui.label(&self.status_message);
            });
            ui.add_space(4.0);
        });

        // --- 3. Left Sidebar (Workspace / Archives) ---
        let workspace_response = egui::Panel::left("workspace").resizable(true).show_inside(ui, |ui| {
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
        
        let workspace_rect = workspace_response.response.rect;

        // --- 4. Main Central Panel (Virtual Tree Items) ---
        egui::CentralPanel::default().show_inside(ui, |ui| {
            
            // Defer execution queues
            let mut trigger_add_files = None;
            let mut trigger_add_folder = None;
            let mut trigger_extract_to = None;

            if let Some(idx) = self.active_archive_idx {
                
                // --- ARCHIVE SCOPE ---
                {
                    let archive = &mut self.archives[idx];
                    
                    ui.horizontal(|ui| {
                        ui.heading(&archive.display_name);
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            
                            let extract_enabled = !self.selected_items.is_empty();
                            if ui.add_enabled(extract_enabled, egui::Button::new("📥 Extract Selected...")).clicked() {
                                let mut dialog = FileDialog::new().set_title("Extract To...");
                                if let Some(dir) = &self.last_browsed_dir {
                                    dialog = dialog.set_directory(dir);
                                }
                                if let Some(out_dir) = dialog.pick_folder() {
                                    trigger_extract_to = Some(out_dir);
                                }
                            }

                            ui.menu_button("➕ Add...", |ui| {
                                if ui.button("📄 Files...").clicked() {
                                    trigger_add_files = Some(active_target_dir.clone());
                                    ui.close();
                                }
                                if ui.button("📁 Folder...").clicked() {
                                    trigger_add_folder = Some(active_target_dir.clone());
                                    ui.close();
                                }
                            });
                            
                            ui.label(egui::RichText::new(format!("Target: /{}", active_target_dir)).weak());
                        });
                    });
                    
                    ui.add_space(4.0);
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

                                let icon = if node.is_dir { if node.expanded { "▼ 📂" } else { "▶ 📁" } } else { "📄" };
                                let label = if node.is_dir {
                                    format!("{} {} ({} items)", icon, node.name, node.item_count)
                                } else {
                                    format!("{} {}", icon, node.name)
                                };
                                
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
                                        if node.is_dir {
                                            if ui.button("Add Files Here...").clicked() {
                                                trigger_add_files = Some(node.path.clone());
                                                ui.close();
                                            }
                                            if ui.button("Add Folder Here...").clicked() {
                                                trigger_add_folder = Some(node.path.clone());
                                                ui.close();
                                            }
                                            ui.separator();
                                        }

                                        if ui.button("Rename").clicked() {
                                            self.renaming_item = Some((node.path.clone(), node.name.clone(), true));
                                            ui.close();
                                        }
                                    }
                                    
                                    if ui.button("Extract Selected...").clicked() {
                                        let mut dialog = FileDialog::new().set_title("Extract To...");
                                        if let Some(dir) = &self.last_browsed_dir {
                                            dialog = dialog.set_directory(dir);
                                        }
                                        if let Some(out_dir) = dialog.pick_folder() {
                                            trigger_extract_to = Some(out_dir);
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
                } // --- END ARCHIVE SCOPE ---

                // Safely execute deferred actions requiring a full &mut self
                if let Some(out_dir) = trigger_extract_to {
                    self.update_last_dir_from_path(&out_dir);
                    let archive = &mut self.archives[idx]; // Re-borrow cleanly here
                    if let Err(e) = archive.extract_items(&self.selected_items, &out_dir) {
                        self.status_message = format!("Extraction failed: {}", e);
                    } else {
                        self.status_message = "Extraction complete!".to_string();
                    }
                }

                if let Some(p) = trigger_add_files { self.manual_add_files_dialog(&p); }
                if let Some(p) = trigger_add_folder { self.manual_add_folder_dialog(&p); }

            } else {
                ui.centered_and_justified(|ui| { ui.label("Open an archive or click 'File -> New Archive' to begin."); });
            }
        });

        // --- 5. Global Drag & Drop Handler ---
        let pointer_pos = ui.ctx().pointer_hover_pos().unwrap_or_default();
        let hovering_workspace = workspace_rect.contains(pointer_pos);

        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            for file in dropped {
                if let Some(path) = file.path {
                    let is_far = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("far"));

                    if is_far && (hovering_workspace || self.active_archive_idx.is_none()) {
                        self.load_archive(path);
                    } else if let Some(idx) = self.active_archive_idx {
                        self.add_disk_item(idx, path, &active_target_dir);
                        self.status_message = format!("Added dropped items to /{}", active_target_dir);
                    }
                }
            }
        }

        let hovered_files = ui.ctx().input(|i| i.raw.hovered_files.clone());
        if !hovered_files.is_empty() {
            request_repaint = true;
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dnd_overlay")));
            
            // Updated to fix the deprecation warning
            let screen_rect = ui.ctx().content_rect();
            
            painter.rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(190));

            let display_text = if hovering_workspace || self.active_archive_idx.is_none() {
                "Drop .far files here to open them"
            } else {
                &format!("Drop files or folders to add to /{}", active_target_dir)
            };

            painter.text(
                screen_rect.center(),
                egui::Align2::CENTER_CENTER,
                display_text,
                egui::FontId::proportional(32.0),
                egui::Color32::WHITE,
            );
        }

        if request_repaint { ui.ctx().request_repaint(); }
    }
}