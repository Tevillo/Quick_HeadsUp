//! TUI flow for the Settings → Import Word List action.
//!
//! Walks the user through:
//!   1. Pick a source file from `imports/`.
//!   2. If the CSV/TSV column layout isn't auto-resolvable (unrecognized
//!      2-column headers, or >2 columns), pick the word column and then
//!      the category column. The category picker includes a
//!      "None — put everything under [General]" option.
//!   3. Type the output filename (default = source stem + `.txt`).
//!   4. If the target exists in `lists/`, resolve the conflict
//!      (overwrite / auto-suffix / cancel).
//!   5. Write into `lists/` and show a success/error screen.
//!
//! Keeping this in its own module avoids bloating `menu.rs` (already
//! ~870 lines and earmarked for a split in v1.4).

use crate::converter::{self, ConvertError, CsvHeaderAnalysis, ParsedList, SourceFormat};
use crate::list_menu::{self, ListKey, ListState};
use crate::paths;
use crate::render::{self, MenuItem};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use std::path::PathBuf;

pub async fn run_import_flow(reader: &mut EventStream) {
    let Some(source_filename) = run_source_picker(reader).await else {
        return;
    };

    let source_path = match paths::imports_dir() {
        Ok(dir) => dir.join(&source_filename),
        Err(e) => {
            show_result(
                &format!("Failed to resolve imports path: {}", e),
                false,
                reader,
            )
            .await;
            return;
        }
    };

    let content = match std::fs::read_to_string(&source_path) {
        Ok(c) => c,
        Err(e) => {
            show_result(
                &format!("Could not read {}: {}", source_filename, e),
                false,
                reader,
            )
            .await;
            return;
        }
    };

    let Some(format) = converter::detect_format(&source_path) else {
        show_result(
            &format!(
                "Unsupported file type: {}. Supported: .csv, .tsv, .json, .txt",
                source_filename
            ),
            false,
            reader,
        )
        .await;
        return;
    };

    let parsed = match parse_with_format(&content, format, reader).await {
        ParseOutcome::Ok(p) => p,
        ParseOutcome::Err(msg) => {
            show_result(&msg, false, reader).await;
            return;
        }
        ParseOutcome::Cancelled => return,
    };

    let default_name = default_output_name(&source_filename);
    let Some(raw_name) = run_output_name_input(&default_name, reader).await else {
        return;
    };
    let output_name = ensure_txt_extension(&raw_name);

    let final_name = match resolve_conflict(&output_name, reader).await {
        ConflictOutcome::Proceed(name) => name,
        ConflictOutcome::Cancel => return,
        ConflictOutcome::Error(msg) => {
            show_result(&msg, false, reader).await;
            return;
        }
    };

    let target_path = match paths::word_file_path(&final_name) {
        Ok(p) => p,
        Err(e) => {
            show_result(
                &format!("Failed to resolve target path: {}", e),
                false,
                reader,
            )
            .await;
            return;
        }
    };

    let body = converter::emit_list(&parsed);
    if let Err(e) = std::fs::write(&target_path, body) {
        show_result(
            &format!("Failed to write {}: {}", final_name, e),
            false,
            reader,
        )
        .await;
        return;
    }

    let (entry_count, category_count) = count_entries(&parsed);
    let msg = format!(
        "Imported {} ({} entries, {} categor{}).",
        final_name,
        entry_count,
        category_count,
        if category_count == 1 { "y" } else { "ies" }
    );
    show_result(&msg, true, reader).await;
}

// ─── Parsing dispatch ───────────────────────────────────────────────

enum ParseOutcome {
    Ok(ParsedList),
    Err(String),
    Cancelled,
}

async fn parse_with_format(
    content: &str,
    format: SourceFormat,
    reader: &mut EventStream,
) -> ParseOutcome {
    match format {
        SourceFormat::Text => to_outcome(converter::parse_text(content)),
        SourceFormat::Json => to_outcome(converter::parse_json(content)),
        SourceFormat::Csv | SourceFormat::Tsv => {
            let delimiter = if format == SourceFormat::Tsv {
                '\t'
            } else {
                ','
            };
            match converter::analyze_csv_headers(content, delimiter) {
                Ok(CsvHeaderAnalysis::AutoResolved {
                    word_col,
                    category_col,
                    ..
                }) => to_outcome(converter::parse_csv_with_cols(
                    content,
                    delimiter,
                    word_col,
                    category_col,
                )),
                Ok(CsvHeaderAnalysis::NeedsPick { headers }) => {
                    let Some(word_col) = run_word_column_picker(&headers, reader).await else {
                        return ParseOutcome::Cancelled;
                    };
                    let Some(category_col) =
                        run_category_column_picker(&headers, word_col, reader).await
                    else {
                        return ParseOutcome::Cancelled;
                    };
                    to_outcome(converter::parse_csv_with_cols(
                        content,
                        delimiter,
                        word_col,
                        category_col,
                    ))
                }
                Err(e) => ParseOutcome::Err(convert_error_message(&e)),
            }
        }
    }
}

fn to_outcome(res: Result<ParsedList, ConvertError>) -> ParseOutcome {
    match res {
        Ok(p) => ParseOutcome::Ok(p),
        Err(e) => ParseOutcome::Err(convert_error_message(&e)),
    }
}

fn convert_error_message(e: &ConvertError) -> String {
    e.to_string()
}

// ─── Screen 1: source picker ────────────────────────────────────────

async fn run_source_picker(reader: &mut EventStream) -> Option<String> {
    let files = match paths::list_available_imports() {
        Ok(f) => f,
        Err(e) => {
            let path = paths::imports_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<install_dir>/imports".to_string());
            show_result(&format!("Could not read {}: {}", path, e), false, reader).await;
            return None;
        }
    };

    if files.is_empty() {
        let path = paths::imports_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<install_dir>/imports".to_string());
        show_empty_imports(&path, reader).await;
        return None;
    }

    let mut state = ListState::new(0);
    let visible = 15usize.min(files.len());

    loop {
        state.ensure_visible(visible, files.len());
        let term_size = render::terminal_size();
        render::render_list_picker(
            "IMPORT — SELECT SOURCE",
            &files,
            state.selected,
            state.scroll_offset,
            term_size,
        );

        let Some(Ok(Event::Key(key))) = reader.next().await else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if crate::input::is_ctrl_c(&key) {
            crate::render::force_exit();
        }
        match list_menu::classify_key(&key) {
            ListKey::Up => state.on_up(files.len()),
            ListKey::Down => state.on_down(files.len()),
            ListKey::Enter => return Some(files[state.selected].clone()),
            ListKey::Cancel => return None,
            ListKey::Unhandled => {}
        }
    }
}

async fn show_empty_imports(path: &str, reader: &mut EventStream) {
    let msg = format!(
        "No files found. Drop a .csv, .tsv, .json, or .txt into:\n{}",
        path
    );
    show_result(&msg, false, reader).await;
}

// ─── Screen 2a: word column picker ──────────────────────────────────

async fn run_word_column_picker(headers: &[String], reader: &mut EventStream) -> Option<usize> {
    let items: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("Column {}: {}", i + 1, h))
        .collect();

    let mut state = ListState::new(0);
    let visible = 15usize.min(items.len());

    loop {
        state.ensure_visible(visible, items.len());
        let term_size = render::terminal_size();
        render::render_list_picker(
            "WHICH COLUMN IS THE WORD?",
            &items,
            state.selected,
            state.scroll_offset,
            term_size,
        );

        let Some(Ok(Event::Key(key))) = reader.next().await else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if crate::input::is_ctrl_c(&key) {
            crate::render::force_exit();
        }
        match list_menu::classify_key(&key) {
            ListKey::Up => state.on_up(items.len()),
            ListKey::Down => state.on_down(items.len()),
            ListKey::Enter => return Some(state.selected),
            ListKey::Cancel => return None,
            ListKey::Unhandled => {}
        }
    }
}

// ─── Screen 2b: category column picker (with "None" option) ─────────
//
// Returns nested `Option` semantics:
//   `None`         — user cancelled with Esc/q (abort the whole import)
//   `Some(None)`   — user picked "None (use General)"
//   `Some(Some(c))` — user picked header column `c`
async fn run_category_column_picker(
    headers: &[String],
    word_col: usize,
    reader: &mut EventStream,
) -> Option<Option<usize>> {
    // Build items + parallel column map so we can translate the list
    // cursor back to either a real column index or the "None" sentinel.
    let mut items: Vec<String> = Vec::new();
    let mut col_map: Vec<Option<usize>> = Vec::new();
    for (i, header) in headers.iter().enumerate() {
        if i == word_col {
            continue;
        }
        items.push(format!("Column {}: {}", i + 1, header));
        col_map.push(Some(i));
    }
    items.push("None — put everything under [General]".to_string());
    col_map.push(None);

    let mut state = ListState::new(0);
    let visible = 15usize.min(items.len());

    loop {
        state.ensure_visible(visible, items.len());
        let term_size = render::terminal_size();
        render::render_list_picker(
            "WHICH COLUMN IS THE CATEGORY?",
            &items,
            state.selected,
            state.scroll_offset,
            term_size,
        );

        let Some(Ok(Event::Key(key))) = reader.next().await else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if crate::input::is_ctrl_c(&key) {
            crate::render::force_exit();
        }
        match list_menu::classify_key(&key) {
            ListKey::Up => state.on_up(items.len()),
            ListKey::Down => state.on_down(items.len()),
            ListKey::Enter => return Some(col_map[state.selected]),
            ListKey::Cancel => return None,
            ListKey::Unhandled => {}
        }
    }
}

// ─── Screen 3: output filename prompt ───────────────────────────────

async fn run_output_name_input(initial: &str, reader: &mut EventStream) -> Option<String> {
    let mut buf = initial.to_string();
    let mut error: Option<&'static str> = None;

    loop {
        let term_size = render::terminal_size();
        let mut items: Vec<MenuItem> = vec![MenuItem::TextInput {
            label: "Output filename:",
            value: &buf,
            editing: true,
        }];
        if let Some(err) = error {
            items.push(MenuItem::Error(err));
        }
        items.push(MenuItem::Label(""));
        items.push(MenuItem::Label("Enter to confirm, Esc to cancel"));
        items.push(MenuItem::Label("(.txt will be appended if missing)"));
        render::render_menu("IMPORT — OUTPUT NAME", &items, 0, term_size);

        let Some(Ok(Event::Key(key))) = reader.next().await else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if crate::input::is_ctrl_c(&key) {
            crate::render::force_exit();
        }
        match key.code {
            KeyCode::Enter => {
                let trimmed = buf.trim();
                if let Some(err) = validate_filename(trimmed) {
                    error = Some(err);
                } else {
                    return Some(trimmed.to_string());
                }
            }
            KeyCode::Esc => return None,
            KeyCode::Backspace => {
                buf.pop();
                error = None;
            }
            KeyCode::Char(c) => {
                buf.push(c);
                error = None;
            }
            _ => {}
        }
    }
}

fn validate_filename(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("Filename cannot be empty");
    }
    if name.contains('/') || name.contains('\\') {
        return Some("Filename cannot contain / or \\");
    }
    if name.contains('\0') {
        return Some("Filename cannot contain null bytes");
    }
    None
}

fn ensure_txt_extension(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".txt") {
        name.to_string()
    } else {
        format!("{}.txt", name)
    }
}

fn default_output_name(source_filename: &str) -> String {
    let stem = match source_filename.rfind('.') {
        Some(pos) => &source_filename[..pos],
        None => source_filename,
    };
    if stem.is_empty() {
        "imported.txt".to_string()
    } else {
        format!("{}.txt", stem)
    }
}

// ─── Screen 4: conflict resolution ──────────────────────────────────

enum ConflictOutcome {
    Proceed(String),
    Cancel,
    Error(String),
}

async fn resolve_conflict(output_name: &str, reader: &mut EventStream) -> ConflictOutcome {
    let target_path = match paths::word_file_path(output_name) {
        Ok(p) => p,
        Err(e) => return ConflictOutcome::Error(format!("Failed to resolve target path: {}", e)),
    };

    if !target_path.exists() {
        return ConflictOutcome::Proceed(output_name.to_string());
    }

    let suffix_name = match find_auto_suffix(output_name) {
        Some(n) => n,
        None => return ConflictOutcome::Error("No free suffix slot up to 999".to_string()),
    };

    let overwrite_label = format!("Overwrite {}", output_name);
    let suffix_label = format!("Use auto-suffix ({})", suffix_name);
    let items: Vec<String> = vec![overwrite_label, suffix_label, "Cancel".to_string()];

    let mut state = ListState::new(1); // default to auto-suffix (safest)
    let visible = items.len();

    loop {
        state.ensure_visible(visible, items.len());
        let term_size = render::terminal_size();
        render::render_list_picker(
            "FILE ALREADY EXISTS",
            &items,
            state.selected,
            state.scroll_offset,
            term_size,
        );

        let Some(Ok(Event::Key(key))) = reader.next().await else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if crate::input::is_ctrl_c(&key) {
            crate::render::force_exit();
        }
        match list_menu::classify_key(&key) {
            ListKey::Up => state.on_up(items.len()),
            ListKey::Down => state.on_down(items.len()),
            ListKey::Enter => {
                return match state.selected {
                    0 => ConflictOutcome::Proceed(output_name.to_string()),
                    1 => ConflictOutcome::Proceed(suffix_name),
                    _ => ConflictOutcome::Cancel,
                };
            }
            ListKey::Cancel => return ConflictOutcome::Cancel,
            ListKey::Unhandled => {}
        }
    }
}

fn find_auto_suffix(name: &str) -> Option<String> {
    let (stem, ext) = match name.rfind('.') {
        Some(pos) => (&name[..pos], &name[pos + 1..]),
        None => (name, ""),
    };
    for i in 1..=999 {
        let candidate = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        let exists = paths::word_file_path(&candidate)
            .map(|p: PathBuf| p.exists())
            .unwrap_or(true);
        if !exists {
            return Some(candidate);
        }
    }
    None
}

// ─── Screen 5: result ────────────────────────────────────────────────

async fn show_result(msg: &str, success: bool, reader: &mut EventStream) {
    let term_size = render::terminal_size();
    render::render_import_result(msg, success, term_size);

    while let Some(Ok(event)) = reader.next().await {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                if crate::input::is_ctrl_c(&key) {
                    crate::render::force_exit();
                }
                return;
            }
        }
    }
}

fn count_entries(parsed: &ParsedList) -> (usize, usize) {
    let entries: usize = parsed.values().map(|v| v.len()).sum();
    (entries, parsed.len())
}
