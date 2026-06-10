use byteorder::{LittleEndian, ReadBytesExt};
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FarEntry {
    pub filename: String,
    pub original_size: u32,
    pub offset: u32,
}

#[derive(Clone, Default)]
pub struct FarArchive {
    pub file_path: Option<PathBuf>,
    pub entries: Vec<FarEntry>,
}

impl FarArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let mut file = File::open(path.as_ref()).map_err(|e| e.to_string())?;
        
        let mut signature = [0u8; 8];
        file.read_exact(&mut signature).map_err(|e| e.to_string())?;
        if &signature != b"FAR!byAZ" {
            return Err("Invalid signature. Not a valid Sims 1 .far file.".into());
        }

        let version = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        if version != 1 {
            return Err(format!("Unsupported FAR version: {}", version));
        }

        let manifest_offset = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(manifest_offset as u64)).map_err(|e| e.to_string())?;
        
        let entry_count = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
        let mut entries = Vec::with_capacity(entry_count as usize);
        
        for _ in 0..entry_count {
            let uncompressed_size = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            let _compressed_size = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            let data_offset = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            
            // FIX: Filename length is 4 bytes (u32), not 2 bytes (u16)
            let filename_len = file.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
            
            let mut filename_bytes = vec![0u8; filename_len as usize];
            file.read_exact(&mut filename_bytes).map_err(|e| format!("Buffer overflow reading filename: {}", e))?;
            
            let filename = String::from_utf8_lossy(&filename_bytes).into_owned();
            
            entries.push(FarEntry {
                filename,
                original_size: uncompressed_size,
                offset: data_offset,
            });
        }
        
        Ok(FarArchive {
            file_path: Some(path.as_ref().to_path_buf()),
            entries,
        })
    }

    pub fn extract_all(&self, output_dir: &Path) -> Result<(), String> {
        let archive_path = self.file_path.as_ref().ok_or("No archive path available")?;
        
        fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

        self.entries.par_iter().try_for_each(|entry| -> Result<(), String> {
            let mut file = File::open(archive_path).map_err(|e| format!("Failed to open archive: {}", e))?;
            file.seek(SeekFrom::Start(entry.offset as u64)).map_err(|e| e.to_string())?;
            
            let mut data = vec![0u8; entry.original_size as usize];
            file.read_exact(&mut data).map_err(|e| format!("Failed reading {}: {}", entry.filename, e))?;
            
            let out_path = output_dir.join(&entry.filename);
            
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            
            let mut out_file = File::create(&out_path).map_err(|e| e.to_string())?;
            out_file.write_all(&data).map_err(|e| e.to_string())?;
            
            Ok(())
        })
    }
}