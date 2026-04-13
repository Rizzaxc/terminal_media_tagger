use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use rusqlite::Connection;
use std::io::{self, stdout};
use std::process::{Command, Stdio};

use crate::{db, search};

pub enum AppMode {
    Browser,
    SearchInput,
    TagCreateInput,
    TagAssign,
    RenameInput,
}

pub struct App {
    pub mode: AppMode,
    pub files: Vec<search::SearchResult>,
    pub file_state: ListState,
    pub tags: Vec<String>,
    pub tag_state: ListState,
    pub input_buffer: String,
    pub error_msg: Option<String>,
    pub query: String,
    pub current_dir: String,
    pub active_file_tags: std::collections::HashSet<String>,
    pub last_key_esc: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Browser,
            files: Vec::new(),
            file_state: ListState::default(),
            tags: Vec::new(),
            tag_state: ListState::default(),
            input_buffer: String::new(),
            error_msg: None,
            query: String::new(),
            current_dir: String::new(),
            active_file_tags: std::collections::HashSet::new(),
            last_key_esc: false,
        }
    }

    pub fn load_files(&mut self, conn: &Connection) {
        let res = if self.query.is_empty() {
            search::browse_dir(conn, &self.current_dir)
        } else {
            search::search_files(conn, &self.query)
        };

        if let Ok(mut r) = res {
            if self.query.is_empty() && !self.current_dir.is_empty() {
                r.insert(0, search::SearchResult {
                    id: -1,
                    rel_path: "..".to_string(),
                    is_dir: true,
                });
            }
            self.files = r;
            if self.files.is_empty() {
                self.file_state.select(None);
            } else {
                let mut idx = self.file_state.selected().unwrap_or(0);
                if idx >= self.files.len() {
                    idx = self.files.len() - 1;
                }
                self.file_state.select(Some(idx));
            }
        }
    }

    pub fn load_tags(&mut self, conn: &Connection) {
        if let Ok(mut stmt) = conn.prepare("SELECT name FROM tags ORDER BY name") {
            if let Ok(rows) = stmt.query_map([], |row| row.get(0)) {
                self.tags = rows.filter_map(|r| r.ok()).collect();
                if self.tags.is_empty() {
                    self.tag_state.select(None);
                } else if self.tag_state.selected().is_none() {
                    self.tag_state.select(Some(0));
                }
            }
        }
    }
}

pub fn run(conn: &mut Connection, target_dir: &std::path::Path) -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = App::new();
    app.load_files(conn);

    let mut should_quit = false;

    while !should_quit {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            app.error_msg = None; // clear generic errors on next key

            let is_esc = key.code == KeyCode::Esc;
            let is_browser = matches!(app.mode, AppMode::Browser);
            if is_esc && is_browser {
                if app.last_key_esc {
                    should_quit = true;
                    continue;
                } else {
                    app.last_key_esc = true;
                    app.error_msg = Some("Press Esc again to quit".to_string());
                }
            } else {
                app.last_key_esc = false;
            }

            match app.mode {
                AppMode::Browser => match key.code {
                    KeyCode::Char('q') => should_quit = true,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = match app.file_state.selected() {
                            Some(i) => if i >= app.files.len().saturating_sub(1) { i } else { i + 1 },
                            None => 0,
                        };
                        app.file_state.select(Some(i));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = match app.file_state.selected() {
                            Some(i) => if i == 0 { 0 } else { i - 1 },
                            None => 0,
                        };
                        app.file_state.select(Some(i));
                    }
                    KeyCode::Char('/') => {
                        app.mode = AppMode::SearchInput;
                        app.input_buffer = app.query.clone();
                    }
                    KeyCode::Char('c') => {
                        app.mode = AppMode::TagCreateInput;
                        app.input_buffer.clear();
                    }
                    KeyCode::Char('s') => {
                        if let Some(i) = app.file_state.selected() {
                            if let Some(f) = app.files.get(i) {
                                let full_path = target_dir.join(&f.rel_path);
                                if f.rel_path == ".." {
                                    let current = if app.current_dir.is_empty() { target_dir.to_path_buf() } else { target_dir.join(&app.current_dir) };
                                    let _ = Command::new("open").arg(&current).spawn();
                                } else {
                                    let _ = Command::new("open").arg("-R").arg(&full_path).spawn();
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(i) = app.file_state.selected() {
                            if let Some(f) = app.files.get(i) {
                                if f.rel_path != ".." {
                                    app.mode = AppMode::RenameInput;
                                    let basename = std::path::Path::new(&f.rel_path).file_name().unwrap_or_default().to_string_lossy().to_string();
                                    app.input_buffer = basename;
                                }
                            }
                        }
                    }
                    KeyCode::Char('t') => {
                        if let Some(f_idx) = app.file_state.selected() {
                            app.load_tags(conn);
                            if let Some(f) = app.files.get(f_idx) {
                                let tags = if f.is_dir {
                                    db::get_tags_for_folder(conn, &f.rel_path).unwrap_or_default()
                                } else {
                                    db::get_tags_for_file(conn, f.id).unwrap_or_default()
                                };
                                app.active_file_tags = tags.into_iter().collect();
                            }
                            app.mode = AppMode::TagAssign;
                        }
                    }
                    KeyCode::Char('p') | KeyCode::Char('a') => {
                        let mut paths = Vec::new();
                        if key.code == KeyCode::Char('a') {
                            for f in &app.files {
                                paths.push(target_dir.join(&f.rel_path));
                            }
                        } else if let Some(i) = app.file_state.selected() {
                            if let Some(f) = app.files.get(i) {
                                paths.push(target_dir.join(&f.rel_path));
                            }
                        }

                        if !paths.is_empty() {
                            let mut cmd = Command::new("vlc");
                            for p in paths {
                                cmd.arg(p);
                            }
                            cmd.stdin(Stdio::null())
                               .stdout(Stdio::null())
                               .stderr(Stdio::null());
                            let _ = cmd.spawn();
                        }
                    }
                    KeyCode::Esc => {
                        if !app.query.is_empty() {
                            app.query.clear();
                            app.load_files(conn);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.file_state.selected() {
                            if let Some(f) = app.files.get(i) {
                                if f.is_dir {
                                    if f.rel_path == ".." {
                                        if let Some(last_slash) = app.current_dir.rfind('/') {
                                            app.current_dir.truncate(last_slash);
                                        } else {
                                            app.current_dir.clear();
                                        }
                                    } else {
                                        app.current_dir = f.rel_path.clone();
                                        app.query.clear();
                                    }
                                    app.file_state.select(Some(0));
                                    app.load_files(conn);
                                } else {
                                    let mut cmd = Command::new("vlc");
                                    cmd.arg(target_dir.join(&f.rel_path));
                                    cmd.stdin(Stdio::null())
                                       .stdout(Stdio::null())
                                       .stderr(Stdio::null());
                                    let _ = cmd.spawn();
                                }
                            }
                        }
                    }
                    _ => {}
                },
                AppMode::SearchInput => match key.code {
                    KeyCode::Enter => {
                        app.query = app.input_buffer.clone();
                        app.load_files(conn);
                        app.mode = AppMode::Browser;
                    }
                    KeyCode::Esc => app.mode = AppMode::Browser,
                    KeyCode::Char(c) => app.input_buffer.push(c),
                    KeyCode::Backspace => { app.input_buffer.pop(); }
                    _ => {}
                },
                AppMode::TagCreateInput => match key.code {
                    KeyCode::Enter => {
                        let input = app.input_buffer.trim();
                        if !input.is_empty() {
                            let parts: Vec<&str> = input.split(':').collect();
                            let res = if parts.len() == 2 {
                                db::add_tag(conn, parts[0].trim(), Some(parts[1].trim()))
                            } else {
                                db::add_tag(conn, parts[0].trim(), None)
                            };
                            if res.is_err() {
                                app.error_msg = Some("Failed to create tag. Parent may not exist.".to_string());
                            } else {
                                app.error_msg = Some("Tag created successfully.".to_string());
                            }
                        }
                        app.mode = AppMode::Browser;
                    }
                    KeyCode::Esc => app.mode = AppMode::Browser,
                    KeyCode::Char(c) => app.input_buffer.push(c),
                    KeyCode::Backspace => { app.input_buffer.pop(); }
                    _ => {}
                },
                AppMode::TagAssign => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => app.mode = AppMode::Browser,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = match app.tag_state.selected() {
                            Some(i) => if i >= app.tags.len().saturating_sub(1) { i } else { i + 1 },
                            None => 0,
                        };
                        app.tag_state.select(Some(i));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = match app.tag_state.selected() {
                            Some(i) => if i == 0 { 0 } else { i - 1 },
                            None => 0,
                        };
                        app.tag_state.select(Some(i));
                    }
                    KeyCode::Enter => {
                        if let Some(f_idx) = app.file_state.selected() {
                            if let Some(t_idx) = app.tag_state.selected() {
                                if let Some(f) = app.files.get(f_idx).cloned() {
                                    let tag_name = app.tags[t_idx].clone();
                                    
                                    let toggled = if f.is_dir {
                                        db::toggle_folder_tag_by_name(conn, &f.rel_path, &tag_name).unwrap_or(false)
                                    } else {
                                        db::toggle_file_tag_by_name(conn, f.id, &tag_name).unwrap_or(false)
                                    };
                                    
                                    if toggled {
                                        app.active_file_tags.insert(tag_name);
                                    } else {
                                        app.active_file_tags.remove(&tag_name);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                AppMode::RenameInput => match key.code {
                    KeyCode::Enter => {
                        let input = app.input_buffer.trim();
                        if !input.is_empty() {
                            if let Some(i) = app.file_state.selected() {
                                if let Some(f) = app.files.get(i).cloned() {
                                    let old_full_path = target_dir.join(&f.rel_path);
                                    let parent = old_full_path.parent();
                                    
                                    if let Some(p) = parent {
                                        let new_full_path = p.join(input);
                                        
                                        match std::fs::rename(&old_full_path, &new_full_path) {
                                            Ok(_) => {
                                                if let Ok(new_rel_path) = new_full_path.strip_prefix(target_dir) {
                                                    let new_rel_str = new_rel_path.to_string_lossy().to_string();
                                                    let _ = db::rename_path(conn, &f.rel_path, &new_rel_str);
                                                    app.error_msg = Some("Renamed successfully.".to_string());
                                                }
                                            }
                                            Err(e) => {
                                                app.error_msg = Some(format!("Rename failed: {}", e));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        app.load_files(conn);
                        app.mode = AppMode::Browser;
                    }
                    KeyCode::Esc => app.mode = AppMode::Browser,
                    KeyCode::Char(c) => app.input_buffer.push(c),
                    KeyCode::Backspace => { app.input_buffer.pop(); }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(3),
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Length(3),
        ])
        .split(f.size());

    let (top_pane, right_pane) = if matches!(app.mode, AppMode::TagAssign) {
        let h_chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([ratatui::layout::Constraint::Percentage(70), ratatui::layout::Constraint::Percentage(30)])
            .split(chunks[0]);
        (h_chunks[0], Some(h_chunks[1]))
    } else {
        (chunks[0], None)
    };

    let items: Vec<ListItem> = app.files.iter().map(|file| {
        let display_name = if file.rel_path == ".." {
            ".. (up)".to_string()
        } else if file.is_dir {
            if app.query.is_empty() {
                let prefix = if app.current_dir.is_empty() { String::new() } else { format!("{}/", app.current_dir) };
                format!("📁 {}", &file.rel_path[prefix.len()..])
            } else {
                format!("📁 {}", file.rel_path)
            }
        } else {
            if app.query.is_empty() {
                let prefix = if app.current_dir.is_empty() { String::new() } else { format!("{}/", app.current_dir) };
                file.rel_path[prefix.len()..].to_string()
            } else {
                file.rel_path.clone()
            }
        };
        ListItem::new(display_name)
    }).collect();

    let title = if app.query.is_empty() && !app.current_dir.is_empty() {
        format!(" Files: {} ", app.current_dir)
    } else if !app.query.is_empty() {
        " Search Results ".to_string()
    } else {
        " Files ".to_string()
    };

    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, top_pane, &mut app.file_state);

    if let Some(rp) = right_pane {
        let tag_items: Vec<ListItem> = app.tags.iter().map(|tag| {
            let prefix = if app.active_file_tags.contains(tag) { "[x]" } else { "[ ]" };
            ListItem::new(format!("{} {}", prefix, tag))
        }).collect();
        let tag_list = List::new(tag_items)
            .block(Block::default().title(" Tags (Enter to toggle) ").borders(Borders::ALL))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ");
        f.render_stateful_widget(tag_list, rp, &mut app.tag_state);
    }

    let input_title = match app.mode {
        AppMode::Browser => " Browser ( /: search | c: create tag | t: tag file | p: play | a: play all | s: show | r: rename | Esc: clear search) ",
        AppMode::SearchInput => " Search Query (Enter to apply, Esc to cancel) ",
        AppMode::TagCreateInput => " Create Tag (Syntax: newTag OR newTag:oldTag) ",
        AppMode::TagAssign => " Tag Assignment (Enter to toggle, Esc to return) ",
        AppMode::RenameInput => " Rename Entry (Enter to confirm, Esc to cancel) ",
    };

    let input_text = match app.mode {
        AppMode::Browser | AppMode::TagAssign => app.query.clone(),
        _ => app.input_buffer.clone() + "█",
    };

    let input_widget = Paragraph::new(input_text)
        .block(Block::default().title(input_title).borders(Borders::ALL));
    f.render_widget(input_widget, chunks[1]);
    
    let info_text = if let Some(ref msg) = app.error_msg {
        msg.clone()
    } else {
        "Status: OK".to_string()
    };
    let info_widget = Paragraph::new(info_text)
        .block(Block::default().title(" Info ").borders(Borders::ALL));
    f.render_widget(info_widget, chunks[2]);
}
