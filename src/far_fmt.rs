use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum FileSource {
    InArchive { original_archive: PathBuf, offset: u32, size: u32 },
    OnDisk(PathBuf),
}

impl FileSource {
    pub fn size(&self) -> u32 {
        match self {
            FileSource::InArchive { size, .. } => *size,
            FileSource::OnDisk(path) => fs::metadata(path).map(|m| m.len() as u32).unwrap_or(0),
        }
    }
}

#[derive(Clone, Default)]
pub struct TreeNode {
    pub is_dir: bool,
    pub expanded: bool,
    pub source: Option<FileSource>,
    pub children: BTreeMap<String, TreeNode>,
}

#[derive(Clone)]
pub struct FlatNode {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    pub size: u32,
    pub item_count: usize, 
}

#[derive(Clone)]
pub struct FarArchive {
    pub display_name: String,
    pub file_path: Option<PathBuf>,
    pub tree_root: TreeNode,
    pub is_modified: bool,
}

impl FarArchive {
    pub fn new_empty(name: String) -> Self {
        Self {
            display_name: name,
            file_path: None,
            tree_root: TreeNode { is_dir: true, expanded: true, ..Default::default() },
            is_modified: false,
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path_buf = path.as_ref().to_path_buf();
        let mut file = File::open(&path_buf).map_err(|e| e.to_string())?;
        
        let mut signature = [0u8; 8];
        file.read_exact(&mut signature).map_err(|e| e.to_string())?;
        if &signature != b"FAR!byAZ" { return Err("Invalid signature.".into()); }

        let version = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        if version != 1 { return Err("Unsupported version.".into()); }

        let manifest_offset = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(manifest_offset as u64)).map_err(|e| e.to_string())?;
        
        let entry_count = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        let mut tree_root = TreeNode { is_dir: true, expanded: true, ..Default::default() };
        
        for _ in 0..entry_count {
            let size = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            let _ = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            let offset = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            let filename_len = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            
            let mut name_bytes = vec![0u8; filename_len as usize];
            file.read_exact(&mut name_bytes).map_err(|e| e.to_string())?;
            let filename = String::from_utf8_lossy(&name_bytes).replace("\\", "/");

            let source = FileSource::InArchive { original_archive: path_buf.clone(), offset, size };
            Self::insert_node(&mut tree_root, &filename, source);
        }
        
        let display_name = path_buf.file_name().unwrap_or_default().to_string_lossy().into_owned();
        Ok(FarArchive { display_name, file_path: Some(path_buf), tree_root, is_modified: false })
    }

    // --- Virtual Tree Operations ---

    pub fn insert_node(root: &mut TreeNode, virtual_path: &str, source: FileSource) {
        let parts: Vec<&str> = virtual_path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = root;
        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            current = current.children.entry(part.to_string()).or_insert_with(|| TreeNode {
                is_dir: !is_last,
                expanded: false,
                source: None,
                children: BTreeMap::new(),
            });
            if is_last {
                current.source = Some(source.clone());
                current.is_dir = false;
            }
        }
    }

    pub fn get_node(&self, virtual_path: &str) -> Option<&TreeNode> {
        let parts: Vec<&str> = virtual_path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &self.tree_root;
        for part in parts {
            current = current.children.get(part)?;
        }
        Some(current)
    }

    pub fn get_node_mut<'a>(&'a mut self, virtual_path: &str) -> Option<&'a mut TreeNode> {
        let parts: Vec<&str> = virtual_path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.tree_root;
        for part in parts {
            current = current.children.get_mut(part)?;
        }
        Some(current)
    }

    pub fn toggle_expansion(&mut self, virtual_path: &str) {
        if let Some(node) = self.get_node_mut(virtual_path) {
            node.expanded = !node.expanded;
        }
    }

    pub fn remove_node(&mut self, virtual_path: &str) -> bool {
        let parts: Vec<&str> = virtual_path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { return false; }
        
        let mut current = &mut self.tree_root;
        for part in &parts[..parts.len() - 1] {
            if let Some(next) = current.children.get_mut(*part) { current = next; } else { return false; }
        }
        
        let removed = current.children.remove(*parts.last().unwrap()).is_some();
        if removed { self.is_modified = true; }
        removed
    }

    pub fn rename_node(&mut self, virtual_path: &str, new_name: &str) -> bool {
        let parts: Vec<&str> = virtual_path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() || new_name.contains('/') || new_name.contains('\\') { return false; }
        
        let mut current = &mut self.tree_root;
        for part in &parts[..parts.len() - 1] {
            if let Some(next) = current.children.get_mut(*part) { current = next; } else { return false; }
        }
        
        if current.children.contains_key(new_name) { return false; }
        
        if let Some(node) = current.children.remove(*parts.last().unwrap()) {
            current.children.insert(new_name.to_string(), node);
            self.is_modified = true;
            return true;
        }
        false
    }

    // --- Saving & Extracting ---

    pub fn save_to_disk(&self, output_path: &Path) -> Result<(), String> {
        // BUG FIX: Prevent truncating the file we are currently reading from.
        // We write to a temporary file in the same directory, then rename it upon success.
        let temp_output_path = output_path.with_extension("tmp_far");

        // Scope the file creation so the handle drops before renaming
        {
            let mut out_file = File::create(&temp_output_path).map_err(|e| e.to_string())?;
            out_file.write_all(b"FAR!byAZ").map_err(|e| e.to_string())?;
            out_file.write_u32::<LittleEndian>(1).map_err(|e| e.to_string())?;
            let manifest_pos = out_file.stream_position().map_err(|e| e.to_string())?;
            out_file.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;

            let mut all_leaves = Vec::new();
            let mut stack = vec![(&self.tree_root, String::new())];
            while let Some((node, path)) = stack.pop() {
                if !node.is_dir { all_leaves.push((path.clone(), node.source.clone())); }
                for (name, child) in &node.children {
                    let child_path = if path.is_empty() { name.clone() } else { format!("{}/{}", path, name) };
                    stack.push((child, child_path));
                }
            }

            let mut manifest_entries = Vec::new();
            for (v_path, source) in all_leaves {
                let offset = out_file.stream_position().map_err(|e| e.to_string())? as u32;
                let mut data = Vec::new();

                if let Some(src) = source {
                    match src {
                        FileSource::InArchive { original_archive, offset: orig_off, size } => {
                            let mut f = File::open(&original_archive).map_err(|e| format!("Failed to read archive: {}", e))?;
                            f.seek(SeekFrom::Start(orig_off as u64)).map_err(|e| e.to_string())?;
                            data.resize(size as usize, 0);
                            f.read_exact(&mut data).map_err(|e| format!("Buffer read error on {}: {}", v_path, e))?;
                        }
                        FileSource::OnDisk(disk_path) => {
                            let mut f = File::open(&disk_path).map_err(|e| e.to_string())?;
                            f.read_to_end(&mut data).map_err(|e| e.to_string())?;
                        }
                    }
                }
                
                let size = data.len() as u32;
                out_file.write_all(&data).map_err(|e| e.to_string())?;
                manifest_entries.push((v_path.replace("/", "\\"), size, offset));
            }

            let manifest_start = out_file.stream_position().map_err(|e| e.to_string())? as u32;
            out_file.write_u32::<LittleEndian>(manifest_entries.len() as u32).map_err(|e| e.to_string())?;
            
            for (filename, size, offset) in manifest_entries {
                out_file.write_u32::<LittleEndian>(size).map_err(|e| e.to_string())?;
                out_file.write_u32::<LittleEndian>(size).map_err(|e| e.to_string())?;
                out_file.write_u32::<LittleEndian>(offset).map_err(|e| e.to_string())?;
                out_file.write_u32::<LittleEndian>(filename.len() as u32).map_err(|e| e.to_string())?;
                out_file.write_all(filename.as_bytes()).map_err(|e| e.to_string())?;
            }

            out_file.seek(SeekFrom::Start(manifest_pos)).map_err(|e| e.to_string())?;
            out_file.write_u32::<LittleEndian>(manifest_start).map_err(|e| e.to_string())?;
        } // out_file is dropped and fully written here

        // Replace the original file atomically
        fs::rename(&temp_output_path, output_path).map_err(|e| {
            let _ = fs::remove_file(&temp_output_path); // Cleanup temp file on failure
            format!("Failed to finalize save: {}", e)
        })?;

        Ok(())
    }

    pub fn extract_all(&self, output_dir: &Path) -> Result<(), String> {
        let mut all_leaves = Vec::new();
        let mut stack = vec![(&self.tree_root, String::new())];
        while let Some((node, path)) = stack.pop() {
            if !node.is_dir { all_leaves.push((path.clone(), node.source.clone())); }
            for (name, child) in &node.children {
                let child_path = if path.is_empty() { name.clone() } else { format!("{}/{}", path, name) };
                stack.push((child, child_path));
            }
        }
        self.extract_leaves(all_leaves, output_dir)
    }

    pub fn extract_items(&self, selected_paths: &HashSet<String>, output_dir: &Path) -> Result<(), String> {
        let mut all_leaves = Vec::new();
        let mut stack = vec![(&self.tree_root, String::new())];
        
        while let Some((node, path)) = stack.pop() {
            if !node.is_dir {
                let is_selected = selected_paths.iter().any(|sel| {
                    path == *sel || path.starts_with(&format!("{}/", sel))
                });
                
                if is_selected {
                    all_leaves.push((path.clone(), node.source.clone()));
                }
            }
            for (name, child) in &node.children {
                let child_path = if path.is_empty() { name.clone() } else { format!("{}/{}", path, name) };
                stack.push((child, child_path));
            }
        }
        
        if all_leaves.is_empty() { return Err("No valid files found to extract in selection.".into()); }
        self.extract_leaves(all_leaves, output_dir)
    }

    fn extract_leaves(&self, leaves: Vec<(String, Option<FileSource>)>, output_dir: &Path) -> Result<(), String> {
        leaves.par_iter().try_for_each(|(v_path, source)| -> Result<(), String> {
            let out_path = output_dir.join(v_path);
            if let Some(parent) = out_path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            
            if let Some(src) = source {
                match src {
                    FileSource::InArchive { original_archive, offset, size } => {
                        let mut f = File::open(original_archive).map_err(|e| e.to_string())?;
                        f.seek(SeekFrom::Start(*offset as u64)).map_err(|e| e.to_string())?;
                        let mut data = vec![0u8; *size as usize];
                        f.read_exact(&mut data).map_err(|e| e.to_string())?;
                        fs::write(&out_path, data).map_err(|e| e.to_string())?;
                    }
                    FileSource::OnDisk(disk_path) => {
                        fs::copy(disk_path, out_path).map_err(|e| e.to_string())?;
                    }
                }
            }
            Ok(())
        })
    }
}