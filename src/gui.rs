#![allow(deprecated)]

use gtk4 as gtk;
use gdk4 as gdk;

use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Button, HeaderBar, ListBox, Orientation, Paned,
    ScrolledWindow, TreeStore, TreeView, TreeViewColumn, CellRendererText, Label, SelectionMode,
    Separator
};
use gdk::{DragAction, FileList, ContentProvider};
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use crate::far_fmt::{FarArchive, FileSource, TreeNode};

// --- App State ---
struct AppState {
    archives: Vec<FarArchive>,
    active_idx: Option<usize>,
    last_browsed_dir: Option<PathBuf>,
}

impl AppState {
    fn update_last_dir(&mut self, path: &PathBuf) {
        if path.is_file() {
            if let Some(parent) = path.parent() { self.last_browsed_dir = Some(parent.to_path_buf()); }
        } else if path.is_dir() {
            self.last_browsed_dir = Some(path.clone());
        }
    }

    fn add_disk_item(&mut self, idx: usize, disk_path: PathBuf, virtual_parent: &str) {
        if disk_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&disk_path) {
                let folder_name = disk_path.file_name().unwrap().to_string_lossy();
                let new_parent = if virtual_parent.is_empty() { folder_name.to_string() } else { format!("{}/{}", virtual_parent, folder_name) };
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
}

// --- UI Sync Helpers ---
fn refresh_workspace(listbox: &ListBox, state: &AppState) {
    // FIX: Safely loop and remove children for GTK 4.10 compatibility
    while let Some(child) = listbox.first_child() {
        listbox.remove(&child);
    }
    
    for (i, archive) in state.archives.iter().enumerate() {
        let prefix = if archive.is_modified { "* " } else { "" };
        let label = Label::builder()
            .label(format!("{}{}", prefix, archive.display_name))
            .halign(gtk::Align::Start)
            .margin_top(5).margin_bottom(5).margin_start(10)
            .build();
        
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&label));
        listbox.append(&row);

        if Some(i) == state.active_idx { listbox.select_row(Some(&row)); }
    }
}

fn populate_tree_store(store: &TreeStore, parent_iter: Option<&gtk::TreeIter>, node: &TreeNode, path: &str) {
    for (name, child) in &node.children {
        let v_path = if path.is_empty() { name.clone() } else { format!("{}/{}", path, name) };
        let icon = if child.is_dir { "📁" } else { "📄" };
        let display_name = format!("{} {}", icon, name);
        let size_str = if child.is_dir { String::new() } else { format!("{} B", child.source.as_ref().map(|s| s.size()).unwrap_or(0)) };
        
        let iter = store.insert_with_values(parent_iter, None, &[(0, &display_name), (1, &size_str), (2, &v_path)]);
        if child.is_dir { populate_tree_store(store, Some(&iter), child, &v_path); }
    }
}

fn refresh_tree(tree_store: &TreeStore, state: &AppState) {
    tree_store.clear();
    if let Some(idx) = state.active_idx {
        populate_tree_store(tree_store, None, &state.archives[idx].tree_root, "");
    }
}

fn get_target_dir(tree_view: &TreeView) -> String {
    let selection = tree_view.selection();
    let (paths, model) = selection.selected_rows();
    if paths.is_empty() { return String::new(); }
    
    let iter = model.iter(&paths[0]).unwrap();
    let v_path: String = model.get_value(&iter, 2).get().unwrap();
    
    let display_name: String = model.get_value(&iter, 0).get().unwrap();
    if display_name.contains("📁") { v_path } 
    else { v_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default() }
}

// --- Main UI Builder ---
pub fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(AppState {
        archives: Vec::new(),
        active_idx: None,
        last_browsed_dir: None,
    }));

    let window = ApplicationWindow::builder().application(app).title("FARxPlus").default_width(1000).default_height(700).build();

    // --- Header Bar ---
    let header_bar = HeaderBar::new();
    window.set_titlebar(Some(&header_bar));

    let btn_new = Button::with_label("New Archive");
    let btn_open = Button::with_label("Open...");
    let btn_save = Button::with_label("Save As...");
    header_bar.pack_start(&btn_new);
    header_bar.pack_start(&btn_open);
    header_bar.pack_end(&btn_save);

    // --- Layout ---
    let main_vbox = Box::new(Orientation::Vertical, 0);
    let paned = Paned::builder().orientation(Orientation::Horizontal).position(250).vexpand(true).build();
    let status_label = Label::builder().label("Ready").halign(gtk::Align::Start).margin_start(10).margin_top(5).margin_bottom(5).build();

    // --- Workspace (Left) ---
    let left_vbox = Box::new(Orientation::Vertical, 0);
    left_vbox.append(&Label::builder().label("Workspace").margin_top(10).margin_bottom(10).build());
    left_vbox.append(&Separator::new(Orientation::Horizontal));

    let archive_list = ListBox::new();
    archive_list.set_selection_mode(SelectionMode::Single);
    let left_scroll = ScrolledWindow::builder().child(&archive_list).vexpand(true).build();
    left_vbox.append(&left_scroll);
    
    let btn_close_archive = Button::with_label("Close Active Archive");
    btn_close_archive.set_margin_top(5);
    btn_close_archive.set_margin_bottom(5);
    btn_close_archive.set_margin_start(5);
    btn_close_archive.set_margin_end(5);
    left_vbox.append(&btn_close_archive);
    paned.set_start_child(Some(&left_vbox));

    // --- Virtual Tree (Right) ---
    let right_vbox = Box::new(Orientation::Vertical, 0);
    
    let toolbar = Box::new(Orientation::Horizontal, 5);
    toolbar.set_margin_top(5);
    toolbar.set_margin_bottom(5);
    toolbar.set_margin_start(5);
    toolbar.set_margin_end(5);
    
    let btn_extract = Button::with_label("📥 Extract Selected");
    let btn_add_files = Button::with_label("➕ Add Files");
    let btn_add_folder = Button::with_label("📁 Add Folder");
    let btn_delete = Button::with_label("🗑 Delete");
    toolbar.append(&btn_extract);
    toolbar.append(&btn_add_files);
    toolbar.append(&btn_add_folder);
    toolbar.append(&Separator::new(Orientation::Vertical));
    toolbar.append(&btn_delete);
    right_vbox.append(&toolbar);

    let tree_store = TreeStore::new(&[glib::Type::STRING, glib::Type::STRING, glib::Type::STRING]);
    let tree_view = TreeView::builder().model(&tree_store).headers_visible(true).vexpand(true).build();

    let name_col = TreeViewColumn::new();
    name_col.set_title("Name");
    let name_cell = CellRendererText::new();
    name_cell.set_editable(true); 
    name_col.pack_start(&name_cell, true);
    name_col.add_attribute(&name_cell, "text", 0);
    tree_view.append_column(&name_col);

    let size_col = TreeViewColumn::new();
    size_col.set_title("Size");
    let size_cell = CellRendererText::new();
    size_col.pack_start(&size_cell, false);
    size_col.add_attribute(&size_cell, "text", 1);
    tree_view.append_column(&size_col);

    let right_scroll = ScrolledWindow::builder().child(&tree_view).build();
    right_vbox.append(&right_scroll);
    paned.set_end_child(Some(&right_vbox));

    main_vbox.append(&paned);
    main_vbox.append(&Separator::new(Orientation::Horizontal));
    main_vbox.append(&status_label);
    window.set_child(Some(&main_vbox));

    // ==========================================
    // ============ SIGNAL HANDLERS =============
    // ==========================================

    // --- 1. Workspace Selection ---
    let state_list = state.clone();
    let tree_store_list = tree_store.clone();
    archive_list.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            // FIX: try_borrow_mut prevents panic when GTK fires this programmatically
            if let Ok(mut s) = state_list.try_borrow_mut() {
                s.active_idx = Some(r.index() as usize);
                refresh_tree(&tree_store_list, &s);
            }
        }
    });

    // --- 2. New Archive ---
    let state_new = state.clone();
    let list_new = archive_list.clone();
    let tree_new = tree_store.clone();
    let status_new = status_label.clone();
    btn_new.connect_clicked(move |_| {
        let mut s = state_new.borrow_mut();
        s.archives.push(FarArchive::new_empty("New_Archive.far".to_string()));
        s.active_idx = Some(s.archives.len() - 1);
        refresh_workspace(&list_new, &s);
        refresh_tree(&tree_new, &s);
        status_new.set_label("Created New Archive.");
    });

    // --- 3. Open Archive ---
    let state_open = state.clone();
    let window_open = window.clone();
    let list_open = archive_list.clone();
    let tree_open = tree_store.clone();
    let status_open = status_label.clone();
    btn_open.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Open FAR Archive");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let filter = gtk::FileFilter::new();
        filter.add_pattern("*.far");
        filters.append(&filter);
        dialog.set_filters(Some(&filters));

        let s = state_open.borrow();
        if let Some(dir) = &s.last_browsed_dir { dialog.set_initial_folder(Some(&gio::File::for_path(dir))); }
        drop(s);

        let state_cb = state_open.clone();
        let list_cb = list_open.clone();
        let tree_cb = tree_open.clone();
        let status_cb = status_open.clone();
        
        dialog.open(Some(&window_open), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let mut s = state_cb.borrow_mut();
                    s.update_last_dir(&path);
                    if let Ok(archive) = FarArchive::open(&path) {
                        s.archives.push(archive);
                        s.active_idx = Some(s.archives.len() - 1);
                        refresh_workspace(&list_cb, &s);
                        refresh_tree(&tree_cb, &s);
                        status_cb.set_label(&format!("Loaded {:?}", path.file_name().unwrap()));
                    }
                }
            }
        });
    });

    // --- 4. Save As ---
    let state_save = state.clone();
    let window_save = window.clone();
    let list_save = archive_list.clone();
    let status_save = status_label.clone();
    btn_save.connect_clicked(move |_| {
        let s = state_save.borrow();
        if let Some(idx) = s.active_idx {
            let dialog = gtk::FileDialog::new();
            dialog.set_initial_name(Some(&s.archives[idx].display_name));
            if let Some(dir) = &s.last_browsed_dir { dialog.set_initial_folder(Some(&gio::File::for_path(dir))); }
            drop(s);

            let state_cb = state_save.clone();
            let list_cb = list_save.clone();
            let status_cb = status_save.clone();
            
            dialog.save(Some(&window_save), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(mut path) = file.path() {
                        if path.extension().unwrap_or_default() != "far" { path.set_extension("far"); }
                        let mut s = state_cb.borrow_mut();
                        s.update_last_dir(&path);
                        let archive = &mut s.archives[idx];
                        match archive.save_to_disk(&path) {
                            Ok(_) => {
                                archive.is_modified = false;
                                archive.file_path = Some(path.clone());
                                archive.display_name = path.file_name().unwrap().to_string_lossy().to_string();
                                refresh_workspace(&list_cb, &s);
                                status_cb.set_label("Archive saved successfully.");
                            }
                            Err(e) => status_cb.set_label(&format!("Save failed: {}", e)),
                        }
                    }
                }
            });
        }
    });

    // --- 5. Extract ---
    let state_ex = state.clone();
    let window_ex = window.clone();
    let tree_view_ex = tree_view.clone();
    let status_ex = status_label.clone();
    btn_extract.connect_clicked(move |_| {
        let s = state_ex.borrow();
        if let Some(idx) = s.active_idx {
            let selection = tree_view_ex.selection();
            let (paths, model) = selection.selected_rows();
            if paths.is_empty() { return; }

            let iter = model.iter(&paths[0]).unwrap();
            let v_path: String = model.get_value(&iter, 2).get().unwrap();
            
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Extract To...");
            if let Some(dir) = &s.last_browsed_dir { dialog.set_initial_folder(Some(&gio::File::for_path(dir))); }
            drop(s);

            let state_cb = state_ex.clone();
            let status_cb = status_ex.clone();
            dialog.select_folder(Some(&window_ex), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(out_dir) = file.path() {
                        let mut s = state_cb.borrow_mut();
                        s.update_last_dir(&out_dir);
                        let mut set = std::collections::HashSet::new();
                        set.insert(v_path.clone());
                        match s.archives[idx].extract_items(&set, &out_dir) {
                            Ok(_) => status_cb.set_label("Extraction complete!"),
                            Err(e) => status_cb.set_label(&format!("Extracted failed: {}", e)),
                        }
                    }
                }
            });
        }
    });

    // --- 6. Add Files & Folders ---
    let setup_add_dialog = |is_folder: bool, window: &ApplicationWindow, tree: &TreeView, state: &Rc<RefCell<AppState>>, list: &ListBox, store: &TreeStore, status: &Label| {
        let s = state.borrow();
        if s.active_idx.is_none() { return; }
        let target_dir = get_target_dir(tree);
        
        let dialog = gtk::FileDialog::new();
        if let Some(dir) = &s.last_browsed_dir { dialog.set_initial_folder(Some(&gio::File::for_path(dir))); }
        drop(s);

        let state_cb = state.clone();
        let list_cb = list.clone();
        let store_cb = store.clone();
        let status_cb = status.clone();

        if is_folder {
            dialog.select_folder(Some(window), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let mut s = state_cb.borrow_mut();
                        let idx = s.active_idx.unwrap();
                        s.update_last_dir(&path);
                        s.add_disk_item(idx, path, &target_dir);
                        refresh_workspace(&list_cb, &s);
                        refresh_tree(&store_cb, &s);
                        status_cb.set_label("Folder added.");
                    }
                }
            });
        } else {
            dialog.open(Some(window), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let mut s = state_cb.borrow_mut();
                        let idx = s.active_idx.unwrap();
                        s.update_last_dir(&path);
                        s.add_disk_item(idx, path, &target_dir);
                        refresh_workspace(&list_cb, &s);
                        refresh_tree(&store_cb, &s);
                        status_cb.set_label("File added.");
                    }
                }
            });
        }
    };

    let w_clone1 = window.clone(); let t_clone1 = tree_view.clone(); let st_clone1 = state.clone(); let l_clone1 = archive_list.clone(); let str_clone1 = tree_store.clone(); let stat_clone1 = status_label.clone();
    btn_add_files.connect_clicked(move |_| setup_add_dialog(false, &w_clone1, &t_clone1, &st_clone1, &l_clone1, &str_clone1, &stat_clone1));
    
    let w_clone2 = window.clone(); let t_clone2 = tree_view.clone(); let st_clone2 = state.clone(); let l_clone2 = archive_list.clone(); let str_clone2 = tree_store.clone(); let stat_clone2 = status_label.clone();
    btn_add_folder.connect_clicked(move |_| setup_add_dialog(true, &w_clone2, &t_clone2, &st_clone2, &l_clone2, &str_clone2, &stat_clone2));

    // --- 7. Delete Item ---
    let state_del = state.clone();
    let tree_view_del = tree_view.clone();
    let list_del = archive_list.clone();
    let tree_store_del = tree_store.clone();
    btn_delete.connect_clicked(move |_| {
        let selection = tree_view_del.selection();
        let (paths, model) = selection.selected_rows();
        if paths.is_empty() { return; }

        let iter = model.iter(&paths[0]).unwrap();
        let v_path: String = model.get_value(&iter, 2).get().unwrap();
        
        let mut s = state_del.borrow_mut();
        if let Some(idx) = s.active_idx {
            s.archives[idx].remove_node(&v_path);
            refresh_workspace(&list_del, &s);
            refresh_tree(&tree_store_del, &s);
        }
    });

    // --- 8. Rename Item (Native Cell Editing) ---
    let state_ren = state.clone();
    let list_ren = archive_list.clone();
    let tree_store_ren = tree_store.clone();
    name_cell.connect_edited(move |_, tree_path, new_name| {
        if new_name.trim().is_empty() { return; }
        
        let iter = tree_store_ren.iter(&tree_path).unwrap();
        let v_path: String = tree_store_ren.get_value(&iter, 2).get().unwrap();
        
        let mut s = state_ren.borrow_mut();
        if let Some(idx) = s.active_idx {
            let clean_name = new_name.replace("📁 ", "").replace("📄 ", "");
            s.archives[idx].rename_node(&v_path, &clean_name);
            refresh_workspace(&list_ren, &s);
            refresh_tree(&tree_store_ren, &s);
        }
    });

    // --- 9. Close Active Archive ---
    let state_close = state.clone();
    let list_close = archive_list.clone();
    let tree_close = tree_store.clone();
    let status_close = status_label.clone();
    btn_close_archive.connect_clicked(move |_| {
        let mut s = state_close.borrow_mut();
        if let Some(idx) = s.active_idx {
            s.archives.remove(idx);
            s.active_idx = if s.archives.is_empty() { None } else { Some(0) };
            refresh_workspace(&list_close, &s);
            refresh_tree(&tree_close, &s);
            status_close.set_label("Archive closed.");
        }
    });

    // --- 10. Drag & Drop ---
    let drop_target = gtk::DropTarget::new(FileList::static_type(), DragAction::COPY);
    let state_drop = state.clone();
    let list_drop = archive_list.clone();
    let tree_store_drop = tree_store.clone();
    let status_drop = status_label.clone();
    let tree_view_drop = tree_view.clone();
    
    drop_target.connect_drop(move |_, value: &glib::Value, _x: f64, _y: f64| -> bool {
        if let Ok(file_list) = value.get::<FileList>() {
            let mut s = state_drop.borrow_mut();
            let target_dir = get_target_dir(&tree_view_drop);
            let files: Vec<gio::File> = file_list.files(); 
            for file in files {
                let opt_path: Option<PathBuf> = file.path();
                if let Some(path) = opt_path {
                    if path.extension().is_some_and(|ext| ext == "far") {
                        if let Ok(archive) = FarArchive::open(&path) {
                            s.archives.push(archive);
                            s.active_idx = Some(s.archives.len() - 1);
                            status_drop.set_label("Loaded Dropped Archive.");
                        }
                    } else if let Some(idx) = s.active_idx {
                        s.add_disk_item(idx, path, &target_dir);
                        status_drop.set_label("Added dropped items.");
                    }
                }
            }
            refresh_workspace(&list_drop, &s);
            refresh_tree(&tree_store_drop, &s);
            return true;
        }
        false
    });
    window.add_controller(drop_target);

    // Exporting Drag out
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(DragAction::COPY);
    let state_drag_out = state.clone();
    let tree_view_drag = tree_view.clone();
    drag_source.connect_prepare(move |_, _x: f64, _y: f64| -> Option<ContentProvider> {
        let selection = tree_view_drag.selection();
        let (selected_paths, model): (Vec<gtk::TreePath>, gtk::TreeModel) = selection.selected_rows();
        if selected_paths.is_empty() { return None; }
        
        let iter = model.iter(&selected_paths[0]).unwrap();
        let v_path: String = model.get_value(&iter, 2).get().unwrap();
        
        let s = state_drag_out.borrow();
        if let Some(idx) = s.active_idx {
            let temp_dir = std::env::temp_dir().join("farxplus_drag");
            let _ = std::fs::create_dir_all(&temp_dir);
            let mut selected_set = std::collections::HashSet::new();
            selected_set.insert(v_path.clone());
            
            if s.archives[idx].extract_items(&selected_set, &temp_dir).is_ok() {
                let extracted_file_path = temp_dir.join(&v_path);
                use glib::prelude::ToValue;
                return Some(ContentProvider::for_value(&gio::File::for_path(extracted_file_path).to_value()));
            }
        }
        None
    });
    tree_view.add_controller(drag_source);

    window.present();
}