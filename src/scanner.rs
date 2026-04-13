use rusqlite::{params, Connection, Result};
use std::path::Path;
use walkdir::WalkDir;

pub fn scan_dir(conn: &mut Connection, root_dir: &Path) -> Result<()> {
    let tx = conn.transaction()?;

    // Purge any hidden files or AppleDouble files that might already be in the database
    tx.execute("DELETE FROM files WHERE rel_path LIKE '.%' OR rel_path LIKE '%/.%'", params![])?;

    let walker = WalkDir::new(root_dir).into_iter().filter_entry(|e| {
        !e.file_name()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    });

    for entry in walker.filter_map(|e| e.ok()) {

        let path = entry.path();

        // Skip image formats explicitly
        let mut is_image = false;
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let e = ext.to_lowercase();
            if matches!(
                e.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "svg" | "ico"
            ) {
                is_image = true;
            }
        }
        if is_image {
            continue;
        }

        if let Ok(rel_path) = path.strip_prefix(root_dir) {
            if rel_path.as_os_str().is_empty() {
                continue; // Root directory itself
            }
            let rel_str = rel_path.to_string_lossy();
            let is_dir = entry.file_type().is_dir();

            tx.execute(
                "INSERT OR IGNORE INTO files (rel_path, is_dir) VALUES (?1, ?2)",
                params![rel_str.as_ref(), is_dir as i32],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
