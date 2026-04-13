mod db;
mod scanner;
mod search;
mod tui;

use rusqlite::Result;
use std::env;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: tui_tagger [--here | --dir <path>]");
        std::process::exit(1);
    }

    let mut target_dir = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--here" => {
                target_dir = Some(env::current_dir().expect("Could not get current directory"));
                i += 1;
            }
            "--dir" => {
                if i + 1 < args.len() {
                    target_dir = Some(Path::new(&args[i+1]).to_path_buf());
                    i += 2;
                } else {
                    eprintln!("Error: --dir requires a path argument.");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Usage: tui_tagger [--here | --dir <path>]");
                std::process::exit(1);
            }
        }
    }

    let target_dir = target_dir.expect("Must provide either --here or --dir <path>");
    let db_path = target_dir.join(".tui-tagger.db");

    let mut conn = db::init_db(&db_path)?;

    // Scan directory and persist to DB
    if let Err(e) = scanner::scan_dir(&mut conn, &target_dir) {
        eprintln!("Failed to scan directory: {}", e);
    }

    // Launch TUI
    if let Err(e) = tui::run(&mut conn, &target_dir) {
        eprintln!("TUI Error: {}", e);
    }

    Ok(())
}
