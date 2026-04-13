use rusqlite::{Connection, Result};

#[derive(Clone)]
pub struct SearchResult {
    pub id: i64,
    pub rel_path: String,
    pub is_dir: bool,
}

pub fn parse_query(query: &str) -> (String, String) {
    if let Some(first_quote) = query.find('"') {
        let split_idx = if first_quote > 0 && query.as_bytes()[first_quote - 1] == b'!' {
            first_quote - 1
        } else {
            first_quote
        };
        (query[..split_idx].trim().to_string(), query[split_idx..].to_string())
    } else {
        // No quotes, everything is filename filter
        (query.trim().to_string(), String::new())
    }
}

pub fn search_files(conn: &Connection, query: &str) -> Result<Vec<SearchResult>> {
    let (filename_filter, tags_query) = parse_query(query);

    let mut sql = String::from("SELECT id, rel_path, is_dir FROM files f WHERE 1=1 ");
    let mut params_vec: Vec<String> = Vec::new();

    if !filename_filter.is_empty() {
        sql.push_str(&format!("AND f.rel_path LIKE ?{} ", params_vec.len() + 1));
        params_vec.push(format!("%{}%", filename_filter));
    }

    if !tags_query.is_empty() {
        // split by ||
        let or_parts: Vec<&str> = tags_query.split("||").collect();
        let mut or_clauses = Vec::new();

        for or_part in or_parts {
            let and_parts: Vec<&str> = or_part.split("&&").collect();
            let mut and_clauses = Vec::new();

            for and_part in and_parts {
                let trimmed = and_part.trim();
                let is_negated = trimmed.starts_with('!');
                let inner = if is_negated {
                    &trimmed[1..]
                } else {
                    trimmed
                };
                let tag = inner.trim_matches('"');
                
                if tag.is_empty() {
                    if is_negated {
                        and_clauses.push("f.id NOT IN (SELECT file_id FROM file_tags)".to_string());
                    }
                    continue;
                }
                
                params_vec.push(tag.to_string());
                let idx = params_vec.len();
                let operator = if is_negated { "NOT IN" } else { "IN" };
                let tag_match = format!("f.id {} (SELECT file_id FROM file_tags ft JOIN tags t ON ft.tag_id = t.id WHERE t.name = ?{idx} OR t.parent_id IN (SELECT id FROM tags WHERE name = ?{idx}))", operator, idx=idx);
                and_clauses.push(tag_match);
            }
            if !and_clauses.is_empty() {
                or_clauses.push(format!("({})", and_clauses.join(" AND ")));
            }
        }

        if !or_clauses.is_empty() {
            sql.push_str("AND (");
            sql.push_str(&or_clauses.join(" OR "));
            sql.push_str(") ");
        }
    }

    sql.push_str("ORDER BY f.rel_path");

    // Rust needs params as a heterogeneous sequence of ToSql, but here it's simple strings.
    // rusqlite allows executing from slice of ToSql.
    let mut stmt = conn.prepare(&sql)?;
    
    // We must pass references
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let rows = stmt.query_map(&params_refs[..], |row| {
        Ok(SearchResult {
            id: row.get(0)?,
            rel_path: row.get(1)?,
            is_dir: row.get(2)?,
        })
    })?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    
    Ok(results)
}

pub fn browse_dir(conn: &Connection, current_dir: &str) -> Result<Vec<SearchResult>> {
    let (sql, params): (&str, Vec<String>) = if current_dir.is_empty() {
        ("SELECT id, rel_path, is_dir FROM files WHERE rel_path NOT LIKE '%/%' ORDER BY is_dir DESC, rel_path COLLATE NOCASE ASC", vec![])
    } else {
        ("SELECT id, rel_path, is_dir FROM files WHERE rel_path LIKE ?1 ORDER BY is_dir DESC, rel_path COLLATE NOCASE ASC", vec![format!("{}/%", current_dir)])
    };
    
    let mut stmt = conn.prepare(sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    
    let rows = stmt.query_map(&params_refs[..], |row| {
        Ok(SearchResult {
            id: row.get(0)?,
            rel_path: row.get(1)?,
            is_dir: row.get(2)?,
        })
    })?;

    let mut results = Vec::new();
    let prefix = if current_dir.is_empty() {
        String::new()
    } else {
        format!("{}/", current_dir)
    };

    for r in rows {
        let res: SearchResult = r?;
        if current_dir.is_empty() {
            results.push(res);
        } else {
            let remainder = &res.rel_path[prefix.len()..];
            if !remainder.contains('/') {
                results.push(res);
            }
        }
    }
    
    Ok(results)
}
