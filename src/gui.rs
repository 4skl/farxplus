use gtk4 as gtk;
use gdk4 as gdk;

use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Button, HeaderBar, ListBox, Orientation, Paned,
    ScrolledWindow, TreeStore, TreeView, TreeViewColumn, CellRendererText, Label, SelectionMode
};
use gdk::{DragAction, FileList, ContentProvider};
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf; // Brought back for explicit typing
use crate::far_fmt::FarArchive;

// Shared state wrapper for GTK callbacks
struct AppState {
    archives: Vec<FarArchive>,
    active_idx: Option<usize>,
}

pub fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(AppState {
        archives: Vec::new(),
        active_idx: None,
    }));

    // --- Window & Layout ---
    let window = ApplicationWindow::builder()
        .application(app)
        .title("FARxPlus")
        .default_width(900)
        .default_height(600)
        .build();

    let header_bar = HeaderBar::new();
    window.set_titlebar(Some(&header_bar));

    let btn_open = Button::with_label("Open Archive");
    let btn_save = Button::with_label("Save As...");
    header_bar.pack_start(&btn_open);
    header_bar.pack_end(&btn_save);

    let main_paned = Paned::builder()
        .orientation(Orientation::Horizontal)
        .position(250)
        .build();

    // --- Left Panel (Workspace) ---
    let left_box = Box::new(Orientation::Vertical, 0);
    let workspace_label = Label::new(Some("Workspace"));
    workspace_label.set_margin_top(10);
    workspace_label.set_margin_bottom(10);
    left_box.append(&workspace_label);

    let archive_list = ListBox::new();
    archive_list.set_selection_mode(SelectionMode::Single);
    
    let left_scroll = ScrolledWindow::builder()
        .child(&archive_list)
        .vexpand(true)
        .build();
    left_box.append(&left_scroll);

    // --- Right Panel (Virtual Tree) ---
    let tree_store = TreeStore::new(&[
        glib::Type::STRING, // Column 0: Icon + Name
        glib::Type::STRING, // Column 1: Size
        glib::Type::STRING, // Column 2: Full Virtual Path (Hidden)
    ]);

    let tree_view = TreeView::builder()
        .model(&tree_store)
        .headers_visible(true)
        .build();

    let name_col = TreeViewColumn::new();
    name_col.set_title("Name");
    let name_cell = CellRendererText::new();
    name_col.pack_start(&name_cell, true);
    name_col.add_attribute(&name_cell, "text", 0);
    tree_view.append_column(&name_col);

    let size_col = TreeViewColumn::new();
    size_col.set_title("Size");
    let size_cell = CellRendererText::new();
    size_col.pack_start(&size_cell, false);
    size_col.add_attribute(&size_cell, "text", 1);
    tree_view.append_column(&size_col);

    let right_scroll = ScrolledWindow::builder()
        .child(&tree_view)
        .vexpand(true)
        .hexpand(true)
        .build();

    main_paned.set_start_child(Some(&left_box));
    main_paned.set_end_child(Some(&right_scroll));
    
    window.set_child(Some(&main_paned));

    // --- Drag and Drop: IN (OS to App) ---
    let drop_target = gtk::DropTarget::new(FileList::static_type(), DragAction::COPY);
    
    let state_clone = state.clone();
    drop_target.connect_drop(move |_, value: &glib::Value, _x: f64, _y: f64| -> bool {
        if let Ok(file_list) = value.get::<FileList>() {
            let mut s = state_clone.borrow_mut();
            
            // FIX: Explicitly enforce the Vec<gio::File> type so Rust stops panicking
            let files: Vec<gio::File> = file_list.files(); 
            
            for file in files {
                // FIX: Explicitly declare the Option<PathBuf>
                let opt_path: Option<PathBuf> = file.path();
                
                if let Some(path) = opt_path {
                    if path.extension().is_some_and(|ext| ext == "far") {
                        if let Ok(archive) = FarArchive::open(&path) {
                            s.archives.push(archive);
                            println!("Loaded archive: {:?}", path);
                        }
                    } else if let Some(idx) = s.active_idx {
                        println!("Would add {:?} to archive {}", path, idx);
                    }
                }
            }
            return true;
        }
        false
    });
    window.add_controller(drop_target);

    // --- Drag and Drop: OUT (App to OS) ---
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(DragAction::COPY);
    
    let state_clone_drag = state.clone();
    let tree_view_clone = tree_view.clone();
    
    drag_source.connect_prepare(move |_, _x: f64, _y: f64| -> Option<ContentProvider> {
        let selection = tree_view_clone.selection();
        let (selected_paths, model): (Vec<gtk::TreePath>, gtk::TreeModel) = selection.selected_rows();
        
        if selected_paths.is_empty() { return None; }
        
        let iter = model.iter(&selected_paths[0]).unwrap();
        
        // FIX: The method is get_value(), not value()
        let v_path: String = model.get_value(&iter, 2).get().unwrap();
        
        let s = state_clone_drag.borrow();
        if let Some(idx) = s.active_idx {
            let archive = &s.archives[idx];
            
            let temp_dir = std::env::temp_dir().join("farxplus_drag");
            let _ = std::fs::create_dir_all(&temp_dir);
            
            let mut selected_set = std::collections::HashSet::new();
            selected_set.insert(v_path.clone());
            
            if archive.extract_items(&selected_set, &temp_dir).is_ok() {
                let extracted_file_path = temp_dir.join(&v_path);
                let gio_file = gio::File::for_path(extracted_file_path);
                
                use glib::prelude::ToValue;
                return Some(ContentProvider::for_value(&gio_file.to_value()));
            }
        }
        None
    });
    tree_view.add_controller(drag_source);

    // --- File Dialog (Native GTK4) ---
    let window_clone = window.clone();
    btn_open.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Open FAR Archive");
        
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("FAR Archives"));
        filter.add_pattern("*.far");
        filters.append(&filter);
        dialog.set_filters(Some(&filters));

        let window_ref = window_clone.clone();
        
        dialog.open(Some(&window_ref), gio::Cancellable::NONE, move |res: Result<gio::File, glib::Error>| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    println!("Native dialog selected: {:?}", path);
                    // Next step: Update UI state with loaded archive!
                }
            }
        });
    });

    window.present();
}