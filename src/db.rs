use rusqlite::{params, Connection, Result};
use std::path::Path;

pub fn init_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;

    // Optimizations to prevent SQLITE_READONLY_DBMOVED on exFAT/FAT32 portable macOS drives
    conn.pragma_update(None, "journal_mode", "MEMORY")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "locking_mode", "EXCLUSIVE")?;


    // Create tables
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            parent_id INTEGER,
            FOREIGN KEY(parent_id) REFERENCES tags(id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            rel_path TEXT UNIQUE NOT NULL,
            is_dir BOOLEAN NOT NULL DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_tags (
            file_id INTEGER,
            tag_id INTEGER,
            PRIMARY KEY (file_id, tag_id),
            FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,
            FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
        )",
        [],
    )?;

    Ok(conn)
}

pub fn add_tag(conn: &Connection, name: &str, parent_name: Option<&str>) -> Result<()> {
    let parent_id: Option<i64> = if let Some(p) = parent_name {
        let mut stmt = conn.prepare("SELECT id FROM tags WHERE name = ?1")?;
        let mut rows = stmt.query(params![p])?;
        if let Some(row) = rows.next()? {
            Some(row.get(0)?)
        } else {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
    } else {
        None
    };

    conn.execute(
        "INSERT OR IGNORE INTO tags (name, parent_id) VALUES (?1, ?2)",
        params![name, parent_id],
    )?;
    Ok(())
}

pub fn get_tags_for_file(conn: &Connection, file_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN file_tags ft ON t.id = ft.tag_id
         WHERE ft.file_id = ?1
         ORDER BY t.name"
    )?;
    let rows = stmt.query_map(params![file_id], |row| row.get(0))?;
    let mut tags = Vec::new();
    for t in rows {
        tags.push(t?);
    }
    Ok(tags)
}

pub fn toggle_file_tag_by_name(conn: &Connection, file_id: i64, tag_name: &str) -> Result<bool> {
    // get tag id
    let mut stmt = conn.prepare("SELECT id FROM tags WHERE name = ?1")?;
    let tag_id: i64 = stmt.query_row(params![tag_name], |row| row.get(0))?;

    // check if it exists
    let exists: i64 = conn.query_row(
        "SELECT count(*) FROM file_tags WHERE file_id = ?1 AND tag_id = ?2",
        params![file_id, tag_id],
        |row| row.get(0)
    )?;

    if exists > 0 {
        conn.execute("DELETE FROM file_tags WHERE file_id = ?1 AND tag_id = ?2", params![file_id, tag_id])?;
        Ok(false)
    } else {
        conn.execute("INSERT INTO file_tags (file_id, tag_id) VALUES (?1, ?2)", params![file_id, tag_id])?;
        Ok(true)
    }
}

pub fn get_tags_for_folder(conn: &Connection, folder_rel_path: &str) -> Result<Vec<String>> {
    let folder_prefix = format!("{}/%", folder_rel_path);
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN file_tags ft ON t.id = ft.tag_id
         JOIN files f ON ft.file_id = f.id
         WHERE f.rel_path LIKE ?1 AND f.is_dir = 0
         GROUP BY t.id
         HAVING COUNT(f.id) = (SELECT COUNT(*) FROM files WHERE rel_path LIKE ?1 AND is_dir = 0)
         ORDER BY t.name"
    )?;
    let rows = stmt.query_map(params![folder_prefix, folder_prefix], |row| row.get(0))?;
    let mut tags = Vec::new();
    for t in rows {
        tags.push(t?);
    }
    Ok(tags)
}

pub fn toggle_folder_tag_by_name(conn: &Connection, folder_rel_path: &str, tag_name: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT id FROM tags WHERE name = ?1")?;
    let tag_id: i64 = stmt.query_row(params![tag_name], |row| row.get(0))?;

    let folder_prefix = format!("{}/%", folder_rel_path);

    let missing_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files f 
         WHERE f.rel_path LIKE ?1 AND f.is_dir = 0 AND f.id NOT IN (
             SELECT file_id FROM file_tags WHERE tag_id = ?2
         )",
        params![folder_prefix, tag_id],
        |row| row.get(0)
    )?;

    if missing_count > 0 {
        conn.execute(
            "INSERT OR IGNORE INTO file_tags (file_id, tag_id)
             SELECT id, ?2 FROM files WHERE rel_path LIKE ?1 AND is_dir = 0",
            params![folder_prefix, tag_id]
        )?;
        Ok(true)
    } else {
        conn.execute(
            "DELETE FROM file_tags 
             WHERE tag_id = ?2 AND file_id IN (
                 SELECT id FROM files WHERE rel_path LIKE ?1 AND is_dir = 0
             )",
             params![folder_prefix, tag_id]
        )?;
        Ok(false)
    }
}

pub fn rename_path(conn: &Connection, old_rel: &str, new_rel: &str) -> Result<()> {
    // exact match
    conn.execute(
        "UPDATE files SET rel_path = ?1 WHERE rel_path = ?2",
        params![new_rel, old_rel],
    )?;
    
    // children match
    let old_prefix = format!("{}/%", old_rel);
    let old_len = old_rel.len() as i32;
    conn.execute(
        "UPDATE files SET rel_path = ?1 || SUBSTR(rel_path, ?2 + 1) WHERE rel_path LIKE ?3",
        params![new_rel, old_len, old_prefix],
    )?;
    
    Ok(())
}

