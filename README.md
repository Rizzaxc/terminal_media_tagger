# TUI Tagger

TUI Tagger is a lightning-fast, portable, terminal-based file tagger designed in Rust. It was built with external drives (FAT32/exFAT) in mind, employing specific SQLite optimizations to track metadata effectively across partitions without locking out standard read/write mechanics on macOS. 

This provides an incredibly lightweight database model allowing structured tagging, rich recursive media directory organization, VLC media integration, and robust search functionality straight from the terminal.

## Getting Started

Run the application by pointing it to your working directory. Ensure you build/run via `cargo`:

```bash
cargo run --release -- --here
# or explicitly state the path
cargo run --release -- --dir /path/to/media
```

## Features and Indexing
- **Database Subsystem:** Operates fully portable via a `.tui-tagger.db` file saved at the top-level of the requested directory. Safe for FAT32/exFAT drives.
- **Smart Scanning:** Booting up automatically scans the directory for new entries and persists them.
  - Automatically filters image formats (`png`, `jpg`, `gif`, `webp`, `tiff`, etc.) to prevent bloating.
  - Hard-skips macOS metadata traces (`._filename`, `.DS_Store`, hidden `.Trash` directories).

## Navigation & Controls 

The TUI works through intuitive keyboard bindings divided by its standard operational modes:

### General Browser Mode

| Key | Action |
| --- | --- |
| `Up` or `k` | Move cursor up. |
| `Down` or `j` | Move cursor down. |
| `Enter` | If focused on a **folder**: Navigates into the folder. <br/>If focused on a **file**: Plays the file immediately via `vlc`. |
| `p` | Plays the currently focused file (or folder's content) via `vlc`. |
| `a` | **Batch Play:** Groups all currently visible queries/files and feeds them as a playlist to `vlc`. |
| `/` | Enter **Search Mode**. |
| `c` | Enter **Tag Creation Mode**. |
| `t` | Enter **Tag Assignment Mode** for the focused item. |
| `q` | Quit the application. |
| `Esc` | Clear the active search query. Pressing `Esc` consecutively quits the app safely. |

### Tagging Ecosystem

Pressing `c` enters Tag Creation Mode.
* Create a standard tag by typing its name and pressing `Enter`.
* **Hierarchical Tags:** You can specify parents instantly using colon syntax: `childTag:parentTag`.

Pressing `t` enters Tag Assignment Mode.
* A sidebar opens showing all database tags. 
* Tags applied to your current selection are checkmarked `[x]`. 
* Use `Up`/`Down` and `Enter` to dynamically toggle the tag. Press `Esc` or `q` to return.
* **Recursive Folder Tagging:** Pressing `Enter` on a directory to toggle a tag evaluates the *entire* directory tree logically: If a single file within the tree lacks the tag, it is assigned broadly to all of them. If all files already own it, it removes the tag recursively from the entire structure. 

### Search Mode (`/`)

Entering search mode allows you to filter the current indexing database logic. Hitting `Enter` applies it, and hitting `Esc` returns you to standard recursive browser navigation.

* First argument before a quotation is matched as a relational path.
* Subsequent string arguments allow for complex logical evaluations matching against the database:
  * Logical ANDing is supported: `tag1&&tag2`
  * Logical ORing is supported: `tag1||tag2`
  * E.g., `folder_name "funny&&video||meme"`

## Requirements
- Rust (`cargo`)
- `vlc` available in your system path (`$PATH`) for media playback functionality.
