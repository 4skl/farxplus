#![allow(deprecated)]

use gtk4 as gtk;
use gdk4 as gdk;

use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Button, HeaderBar, ListBox, Orientation, Paned,
    ScrolledWindow, Label, SelectionMode, Separator, SearchEntry
};
use gdk::{DragAction, FileList, ContentProvider};
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use crate::far_fmt::{FarArchive, FileSource, TreeNode};

// ==========================================
// ====== CUSTOM GTK4 DATA MODEL ============
// ==========================================

mod archive_node {
    use gio;
    use glib;
    use glib::subclass::prelude::*;
    use std::cell::{Cell, RefCell};

    mod imp {
        use super::*;

        #[derive(Default)]
        pub struct ArchiveNodeObject {
            pub name: RefCell<String>,
            pub size_str: RefCell<String>,
            pub v_path: RefCell<String>,
            pub icon: RefCell<String>,
            pub is_dir: Cell<bool>,
            pub children: RefCell<Option<gio::ListStore>>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for ArchiveNodeObject {
            const NAME: &'static str = "ArchiveNodeObject";
            type Type = super::ArchiveNodeObject;
        }

        impl ObjectImpl for ArchiveNodeObject {}
    }

    glib::wrapper! {
        pub struct ArchiveNodeObject(ObjectSubclass<imp::ArchiveNodeObject>);
    }

    impl ArchiveNodeObject {
        pub fn new(name: &str, size_str: &str, v_path: &str, icon: &str, is_dir: bool, children: Option<gio::ListStore>) -> Self {
            let obj: Self = glib::Object::builder().build();
            let imp = obj.imp();
            imp.name.replace(name.to_string());
            imp.size_str.replace(size_str.to_string());
            imp.v_path.replace(v_path.to_string());
            imp.icon.replace(icon.to_string());
            imp.is_dir.set(is_dir);
            *imp.children.borrow_mut() = children;
            obj
        }
        pub fn name(&self) -> String { self.imp().name.borrow().clone() }
        pub fn size_str(&self) -> String { self.imp().size_str.borrow().clone() }
        pub fn v_path(&self) -> String { self.imp().v_path.borrow().clone() }
        pub fn icon(&self) -> String { self.imp().icon.borrow().clone() }
        pub fn is_dir(&self) -> bool { self.imp().is_dir.get() }
        pub fn children(&self) -> Option<gio::ListStore> { self.imp().children.borrow().clone() }
    }
}
use archive_node::ArchiveNodeObject;

// ==========================================
// ============= APP STATE ==================
// ==========================================

struct AppState {
    archives: Vec<FarArchive>,
    active_idx: Option<usize>,
    last_browsed_dir: Option<PathBuf>,
    search_query: String,
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
        let filename = match disk_path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => return, 
        };

        if disk_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&disk_path) {
                let new_parent = if virtual_parent.is_empty() { filename } else { format!("{}/{}", virtual_parent, filename) };
                for entry in entries.flatten() {
                    self.add_disk_item(idx, entry.path(), &new_parent);
                }
            }
        } else {
            let v_path = if virtual_parent.is_empty() { filename } else { format!("{}/{}", virtual_parent, filename) };
            FarArchive::insert_node(&mut self.archives[idx].tree_root, &v_path, FileSource::OnDisk(disk_path));
            self.archives[idx].is_modified = true;
        }
    }
}

// ==========================================
// ============ UI SYNC HELPERS =============
// ==========================================

fn refresh_workspace(listbox: &ListBox, state: &AppState) {
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

fn build_list_store(node: &TreeNode, path: &str, query: &str) -> Option<gio::ListStore> {
    let store = gio::ListStore::new::<ArchiveNodeObject>();
    let mut has_match = false;
    
    for (name, child) in &node.children {
        let v_path = if path.is_empty() { name.clone() } else { format!("{}/{}", path, name) };
        let matches_query = query.is_empty() || name.to_lowercase().contains(&query.to_lowercase());
        
        let icon = if child.is_dir { "📁" } else { "📄" };
        let size_str = if child.is_dir { String::new() } else { format!("{} B", child.source.as_ref().map(|s| s.size()).unwrap_or(0)) };
        
        let child_store = if child.is_dir { build_list_store(child, &v_path, query) } else { None };
        let children_match = child_store.is_some() && child_store.as_ref().unwrap().n_items() > 0;

        if query.is_empty() || matches_query || children_match {
            let obj = ArchiveNodeObject::new(name, &size_str, &v_path, icon, child.is_dir, child_store);
            store.append(&obj);
            has_match = true;
        }
    }
    
    if has_match { Some(store) } else { None }
}

fn refresh_tree(root_store: &gio::ListStore, state: &AppState) {
    root_store.remove_all();
    if let Some(idx) = state.active_idx {
        if let Some(store) = build_list_store(&state.archives[idx].tree_root, "", &state.search_query) {
            for i in 0..store.n_items() {
                if let Some(item) = store.item(i) {
                    root_store.append(&item);
                }
            }
        }
    }
}

fn get_selected_object(selection_model: &gtk::SingleSelection) -> Option<ArchiveNodeObject> {
    let item = selection_model.selected_item()?;
    let list_row = item.downcast::<gtk::TreeListRow>().ok()?;
    list_row.item()?.downcast::<ArchiveNodeObject>().ok()
}

fn get_target_dir(selection_model: &gtk::SingleSelection) -> String {
    if let Some(obj) = get_selected_object(selection_model) {
        let v_path = obj.v_path();
        if obj.is_dir() { return v_path; } 
        else { return v_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default(); }
    }
    String::new()
}

// ==========================================
// ============ MAIN UI BUILDER =============
// ==========================================

pub fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(AppState {
        archives: Vec::new(),
        active_idx: None,
        last_browsed_dir: None,
        search_query: String::new(),
    }));

    let window = ApplicationWindow::builder().application(app).title("FARxPlus").default_width(1000).default_height(700).build();

    let header_bar = HeaderBar::new();
    window.set_titlebar(Some(&header_bar));

    let btn_new = Button::with_label("New Archive");
    let btn_open = Button::with_label("Open...");
    let btn_save = Button::with_label("Save As...");
    header_bar.pack_start(&btn_new);
    header_bar.pack_start(&btn_open);
    header_bar.pack_end(&btn_save);

    let main_vbox = Box::new(Orientation::Vertical, 0);
    let paned = Paned::builder().orientation(Orientation::Horizontal).position(250).vexpand(true).build();
    let status_label = Label::builder().label("Ready").halign(gtk::Align::Start).margin_start(10).margin_top(5).margin_bottom(5).build();

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

    let right_vbox = Box::new(Orientation::Vertical, 0);
    
    let toolbar = Box::new(Orientation::Horizontal, 5);
    toolbar.set_margin_top(5);
    toolbar.set_margin_bottom(5);
    toolbar.set_margin_start(5);
    toolbar.set_margin_end(5);
    
    let btn_extract = Button::with_label("📥 Extract Selected");
    let btn_add_files = Button::with_label("➕ Add Files");
    let btn_add_folder = Button::with_label("📁 Add Folder");
    let btn_rename = Button::with_label("✏️ Rename"); 
    let btn_delete = Button::with_label("🗑 Delete");
    toolbar.append(&btn_extract);
    toolbar.append(&btn_add_files);
    toolbar.append(&btn_add_folder);
    toolbar.append(&Separator::new(Orientation::Vertical));
    toolbar.append(&btn_rename);
    toolbar.append(&btn_delete);
    right_vbox.append(&toolbar);

    let search_bar = SearchEntry::new();
    search_bar.set_placeholder_text(Some("Search files..."));
    search_bar.set_margin_start(5);
    search_bar.set_margin_end(5);
    search_bar.set_margin_bottom(5);
    right_vbox.append(&search_bar);

    // ==========================================
    // ====== MODERN COLUMNVIEW SETUP ===========
    // ==========================================
    let root_store = gio::ListStore::new::<ArchiveNodeObject>();
    
    let tree_list_model = gtk::TreeListModel::new(
        root_store.clone(),
        false, 
        false, 
        |item| {
            let obj = item.downcast_ref::<ArchiveNodeObject>().unwrap();
            obj.children().map(|s| s.upcast::<gio::ListModel>())
        }
    );

    let selection_model = gtk::SingleSelection::new(Some(tree_list_model));
    let column_view = gtk::ColumnView::new(Some(selection_model.clone()));

    // Column 1: Name & Icon
    let factory_name = gtk::SignalListItemFactory::new();
    factory_name.connect_setup(|_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        let expander = gtk::TreeExpander::new();
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 6); // 6px gap between icon and text
        
        // UX FIX: Add vertical padding to make the row taller. 
        // A taller row forces the expander arrow to generate a much larger, easier-to-click hitbox!
        box_.set_margin_top(4);
        box_.set_margin_bottom(4);

        let icon_label = gtk::Label::new(None);
        let name_label = gtk::Label::new(None);
        
        // Prevent extremely long file names from stretching the window horizontally
        name_label.set_ellipsize(gtk::pango::EllipsizeMode::End); 
        
        box_.append(&icon_label);
        box_.append(&name_label);
        expander.set_child(Some(&box_));
        list_item.set_child(Some(&expander));
    });
    
    factory_name.connect_bind(|_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        let expander = list_item.child().unwrap().downcast::<gtk::TreeExpander>().unwrap();
        let list_row = list_item.item().unwrap().downcast::<gtk::TreeListRow>().unwrap();
        expander.set_list_row(Some(&list_row)); 
        
        let item_obj = list_row.item().unwrap().downcast::<ArchiveNodeObject>().unwrap();
        let box_ = expander.child().unwrap().downcast::<gtk::Box>().unwrap();
        let icon_label = box_.first_child().unwrap().downcast::<gtk::Label>().unwrap();
        let name_label = icon_label.next_sibling().unwrap().downcast::<gtk::Label>().unwrap();
        
        icon_label.set_text(&item_obj.icon());
        name_label.set_text(&item_obj.name());
    });
    
    let col_name = gtk::ColumnViewColumn::new(Some("Name"), Some(factory_name));
    col_name.set_expand(true);
    column_view.append_column(&col_name);

    // Column 2: Size
    let factory_size = gtk::SignalListItemFactory::new();
    factory_size.connect_setup(|_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder().halign(gtk::Align::Start).build();
        list_item.set_child(Some(&label));
    });
    
    factory_size.connect_bind(|_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        let label = list_item.child().unwrap().downcast::<gtk::Label>().unwrap();
        let list_row = list_item.item().unwrap().downcast::<gtk::TreeListRow>().unwrap();
        let item_obj = list_row.item().unwrap().downcast::<ArchiveNodeObject>().unwrap();
        label.set_text(&item_obj.size_str());
    });

    let col_size = gtk::ColumnViewColumn::new(Some("Size"), Some(factory_size));
    column_view.append_column(&col_size);

    // HEIGHT FIX: Add .vexpand(true) so the window stretches to fill the screen
    let right_scroll = ScrolledWindow::builder()
        .child(&column_view)
        .vexpand(true) 
        .build();
    
    right_vbox.append(&right_scroll);
    paned.set_end_child(Some(&right_vbox));

    main_vbox.append(&paned);
    main_vbox.append(&Separator::new(Orientation::Horizontal));
    main_vbox.append(&status_label);
    window.set_child(Some(&main_vbox));

    // ==========================================
    // ============ SIGNAL HANDLERS =============
    // ==========================================

    column_view.connect_activate(|view, position| {
        let selection = view.model().unwrap().downcast::<gtk::SingleSelection>().unwrap();
        if let Some(item) = selection.item(position) {
            if let Ok(row) = item.downcast::<gtk::TreeListRow>() {
                row.set_expanded(!row.is_expanded());
            }
        }
    });

    let state_search = state.clone();
    let root_store_search = root_store.clone();
    search_bar.connect_search_changed(move |entry| {
        let text = entry.text().to_string();
        if let Ok(mut s) = state_search.try_borrow_mut() {
            s.search_query = text;
            refresh_tree(&root_store_search, &s);
        }
    });

    let state_list = state.clone();
    let root_store_list = root_store.clone();
    archive_list.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            if let Ok(mut s) = state_list.try_borrow_mut() {
                s.active_idx = Some(r.index() as usize);
                refresh_tree(&root_store_list, &s);
            }
        }
    });

    let state_new = state.clone();
    let list_new = archive_list.clone();
    let root_store_new = root_store.clone();
    let status_new = status_label.clone();
    btn_new.connect_clicked(move |_| {
        if let Ok(mut s) = state_new.try_borrow_mut() {
            s.archives.push(FarArchive::new_empty("New_Archive.far".to_string()));
            s.active_idx = Some(s.archives.len() - 1);
            refresh_workspace(&list_new, &s);
            refresh_tree(&root_store_new, &s);
            status_new.set_label("Created New Archive.");
        }
    });

    let state_open = state.clone();
    let window_open = window.clone();
    let list_open = archive_list.clone();
    let root_store_open = root_store.clone();
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
        let root_store_cb = root_store_open.clone();
        let status_cb = status_open.clone();
        
        dialog.open(Some(&window_open), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    if let Ok(mut s) = state_cb.try_borrow_mut() {
                        s.update_last_dir(&path);
                        if let Ok(archive) = FarArchive::open(&path) {
                            s.archives.push(archive);
                            s.active_idx = Some(s.archives.len() - 1);
                            refresh_workspace(&list_cb, &s);
                            refresh_tree(&root_store_cb, &s);
                            status_cb.set_label(&format!("Loaded {:?}", path.file_name().unwrap()));
                        }
                    }
                }
            }
        });
    });

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
            let archive_clone = s.archives[idx].clone();
            drop(s);

            let state_cb = state_save.clone();
            let list_cb = list_save.clone();
            let status_cb = status_save.clone();
            
            dialog.save(Some(&window_save), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(mut path) = file.path() {
                        if path.extension().unwrap_or_default() != "far" { path.set_extension("far"); }
                        if let Ok(mut s) = state_cb.try_borrow_mut() { s.update_last_dir(&path); }
                        
                        status_cb.set_label("⏳ Saving archive...");
                        
                        let (tx, rx) = std::sync::mpsc::channel();
                        let path_clone = path.clone();
                        
                        std::thread::spawn(move || {
                            let result = archive_clone.save_to_disk(&path_clone);
                            let _ = tx.send(result);
                        });

                        let state_cb_timer = state_cb.clone();
                        let list_cb_timer = list_cb.clone();
                        let status_cb_timer = status_cb.clone();
                        let path_timer = path.clone();

                        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                            match rx.try_recv() {
                                Ok(result) => {
                                    if let Ok(mut s) = state_cb_timer.try_borrow_mut() {
                                        if let Some(idx) = s.active_idx {
                                            match result {
                                                Ok(_) => {
                                                    s.archives[idx].is_modified = false;
                                                    s.archives[idx].file_path = Some(path_timer.clone());
                                                    s.archives[idx].display_name = path_timer.file_name().unwrap().to_string_lossy().to_string();
                                                    refresh_workspace(&list_cb_timer, &s);
                                                    status_cb_timer.set_label("✅ Archive saved successfully.");
                                                }
                                                Err(e) => status_cb_timer.set_label(&format!("❌ Save failed: {}", e)),
                                            }
                                        }
                                    }
                                    glib::ControlFlow::Break
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    status_cb_timer.set_label("❌ Save failed (thread disconnected).");
                                    glib::ControlFlow::Break
                                }
                            }
                        });
                    }
                }
            });
        }
    });

    let state_ex = state.clone();
    let window_ex = window.clone();
    let selection_ex = selection_model.clone();
    let status_ex = status_label.clone();
    btn_extract.connect_clicked(move |_| {
        let s = state_ex.borrow();
        if let Some(idx) = s.active_idx {
            if let Some(obj) = get_selected_object(&selection_ex) {
                let v_path = obj.v_path();
                let archive_clone = s.archives[idx].clone();
                
                let dialog = gtk::FileDialog::new();
                dialog.set_title("Extract To...");
                if let Some(dir) = &s.last_browsed_dir { dialog.set_initial_folder(Some(&gio::File::for_path(dir))); }
                drop(s);

                let state_cb = state_ex.clone();
                let status_cb = status_ex.clone();
                dialog.select_folder(Some(&window_ex), gio::Cancellable::NONE, move |res| {
                    if let Ok(file) = res {
                        if let Some(out_dir) = file.path() {
                            if let Ok(mut s) = state_cb.try_borrow_mut() { s.update_last_dir(&out_dir); }
                            
                            status_cb.set_label("⏳ Extracting files...");
                            let mut set = std::collections::HashSet::new();
                            set.insert(v_path.clone());
                            
                            let (tx, rx) = std::sync::mpsc::channel();
                            let status_thread_cb = status_cb.clone();
                            
                            std::thread::spawn(move || {
                                let result = archive_clone.extract_items(&set, &out_dir);
                                let _ = tx.send(result);
                            });

                            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                                match rx.try_recv() {
                                    Ok(result) => {
                                        match result {
                                            Ok(_) => status_thread_cb.set_label("✅ Extraction complete!"),
                                            Err(e) => status_thread_cb.set_label(&format!("❌ Extract failed: {}", e)),
                                        }
                                        glib::ControlFlow::Break
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        status_thread_cb.set_label("❌ Extract failed (thread disconnected).");
                                        glib::ControlFlow::Break
                                    }
                                }
                            });
                        }
                    }
                });
            }
        }
    });

    let setup_add_dialog = |is_folder: bool, window: &ApplicationWindow, sel_model: &gtk::SingleSelection, state: &Rc<RefCell<AppState>>, list: &ListBox, store: &gio::ListStore, status: &Label| {
        let s = state.borrow();
        if s.active_idx.is_none() { return; }
        let target_dir = get_target_dir(sel_model);
        
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
                        if let Ok(mut s) = state_cb.try_borrow_mut() {
                            let idx = s.active_idx.unwrap();
                            s.update_last_dir(&path);
                            s.add_disk_item(idx, path, &target_dir);
                            refresh_workspace(&list_cb, &s);
                            refresh_tree(&store_cb, &s);
                            status_cb.set_label("Folder added.");
                        }
                    }
                }
            });
        } else {
            dialog.open(Some(window), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        if let Ok(mut s) = state_cb.try_borrow_mut() {
                            let idx = s.active_idx.unwrap();
                            s.update_last_dir(&path);
                            s.add_disk_item(idx, path, &target_dir);
                            refresh_workspace(&list_cb, &s);
                            refresh_tree(&store_cb, &s);
                            status_cb.set_label("File added.");
                        }
                    }
                }
            });
        }
    };

    let w_clone1 = window.clone(); let sel_clone1 = selection_model.clone(); let st_clone1 = state.clone(); let l_clone1 = archive_list.clone(); let r_clone1 = root_store.clone(); let stat_clone1 = status_label.clone();
    btn_add_files.connect_clicked(move |_| setup_add_dialog(false, &w_clone1, &sel_clone1, &st_clone1, &l_clone1, &r_clone1, &stat_clone1));
    
    let w_clone2 = window.clone(); let sel_clone2 = selection_model.clone(); let st_clone2 = state.clone(); let l_clone2 = archive_list.clone(); let r_clone2 = root_store.clone(); let stat_clone2 = status_label.clone();
    btn_add_folder.connect_clicked(move |_| setup_add_dialog(true, &w_clone2, &sel_clone2, &st_clone2, &l_clone2, &r_clone2, &stat_clone2));

    let state_del = state.clone();
    let selection_del = selection_model.clone();
    let list_del = archive_list.clone();
    let root_store_del = root_store.clone();
    btn_delete.connect_clicked(move |_| {
        if let Some(obj) = get_selected_object(&selection_del) {
            let v_path = obj.v_path();
            if let Ok(mut s) = state_del.try_borrow_mut() {
                if let Some(idx) = s.active_idx {
                    s.archives[idx].remove_node(&v_path);
                    refresh_workspace(&list_del, &s);
                    refresh_tree(&root_store_del, &s);
                }
            }
        }
    });

    let state_ren = state.clone();
    let window_ren = window.clone();
    let selection_ren = selection_model.clone();
    let list_ren = archive_list.clone();
    let root_store_ren = root_store.clone();
    
    btn_rename.connect_clicked(move |_| {
        if let Some(obj) = get_selected_object(&selection_ren) {
            let v_path = obj.v_path();
            let current_name = obj.name();

            let dialog = gtk::Window::builder()
                .title("Rename")
                .modal(true)
                .transient_for(&window_ren)
                .default_width(300)
                .build();

            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
            vbox.set_margin_start(10); vbox.set_margin_end(10);
            vbox.set_margin_top(10); vbox.set_margin_bottom(10);

            let entry = gtk::Entry::new();
            entry.set_text(&current_name);
            vbox.append(&entry);

            let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 5);
            btn_box.set_halign(gtk::Align::End);
            let btn_cancel = gtk::Button::with_label("Cancel");
            let btn_ok = gtk::Button::with_label("Rename");
            btn_box.append(&btn_cancel);
            btn_box.append(&btn_ok);
            vbox.append(&btn_box);

            dialog.set_child(Some(&vbox));

            let d_clone1 = dialog.clone();
            btn_cancel.connect_clicked(move |_| d_clone1.destroy());

            let d_clone2 = dialog.clone();
            let state_ok = state_ren.clone();
            let list_ok = list_ren.clone();
            let root_ok = root_store_ren.clone();
            
            btn_ok.connect_clicked(move |_| {
                let new_name = entry.text().to_string();
                if !new_name.trim().is_empty() {
                    if let Ok(mut s) = state_ok.try_borrow_mut() {
                        if let Some(idx) = s.active_idx {
                            if s.archives[idx].rename_node(&v_path, &new_name) {
                                refresh_workspace(&list_ok, &s);
                                refresh_tree(&root_ok, &s);
                            }
                        }
                    }
                }
                d_clone2.destroy();
            });

            dialog.present();
        }
    });

    let state_close = state.clone();
    let list_close = archive_list.clone();
    let root_store_close = root_store.clone();
    let status_close = status_label.clone();
    btn_close_archive.connect_clicked(move |_| {
        if let Ok(mut s) = state_close.try_borrow_mut() {
            if let Some(idx) = s.active_idx {
                s.archives.remove(idx);
                s.active_idx = if s.archives.is_empty() { None } else { Some(0) };
                refresh_workspace(&list_close, &s);
                refresh_tree(&root_store_close, &s);
                status_close.set_label("Archive closed.");
            }
        }
    });

    let drop_target = gtk::DropTarget::new(FileList::static_type(), DragAction::COPY);
    let state_drop = state.clone();
    let list_drop = archive_list.clone();
    let root_store_drop = root_store.clone();
    let status_drop = status_label.clone();
    let selection_drop = selection_model.clone();
    
    drop_target.connect_drop(move |_, value: &glib::Value, _x: f64, _y: f64| -> bool {
        if let Ok(file_list) = value.get::<FileList>() {
            if let Ok(mut s) = state_drop.try_borrow_mut() {
                let target_dir = get_target_dir(&selection_drop);
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
                refresh_tree(&root_store_drop, &s);
                return true;
            }
        }
        false
    });
    window.add_controller(drop_target);

    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(DragAction::COPY);
    let state_drag_out = state.clone();
    let selection_drag_out = selection_model.clone();
    drag_source.connect_prepare(move |_, _x: f64, _y: f64| -> Option<ContentProvider> {
        if let Some(obj) = get_selected_object(&selection_drag_out) {
            let v_path = obj.v_path();
            if let Ok(s) = state_drag_out.try_borrow() {
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
            }
        }
        None
    });
    column_view.add_controller(drag_source);

    window.present();
}