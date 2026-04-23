# Custom Word List Support — v1.2 Plan

Implements the two v1.2 TODO items:

1. **Custom word list support** (TODO.md:24) — user-facing in-TUI flow to produce `lists/`-compatible `.txt` files from external sources.
2. **In-TUI word-list converter** (TODO.md:33) — the converter engine that does the actual parsing, with unit tests.

These are one combined feature: a "Import Word List" action in Settings → pick a source file from a new `imports/` sibling dir → convert it → write into `lists/`.

## Clarified design decisions

From Q&A with the user:

| # | Decision |
|---|---|
| 1 | **Converter only** — no separate in-TUI manual-entry creator. |
| 2 | **Source discovery = drop-in dir.** Users drop files into `./imports/` (new sibling of `lists/`, `.history/`) and pick from a list in the TUI. No free-form path input. |
| 3 | **Output filename** — user prompted in TUI for output name; default = source stem + `.txt`. If target exists in `lists/`, prompt with `Overwrite` / `Auto-suffix` / `Cancel`. Auto-suffix appends `_1`, `_2`, … before `.txt`. |
| 4 | **CSV/TSV parsing** — max 2 columns (word, category). Word column auto-detected when its header is `word`, `name`, `entry`, or `term` (case-insensitive). If neither column matches, TUI prompts user to pick which column is the word. Single-column files → all words under a `[General]` category. |
| 5 | **JSON shape** — `{ "Category A": ["word1", "word2"], "Category B": [...] }`. Only this shape is accepted. |
| 6 | **Tests** — unit tests for the converter module only (scoped to converter). Broader test layer stays in v1.4. |
| 7 | Plan stored at `.claude/custom-word-list-plan.md` (this file); `/clear` before implementation. |

## Scope — in & out

**In scope:**
- New `imports/` sibling dir, auto-created on first use.
- New in-app action: `Settings → Import Word List`.
- Parsers for `.csv`, `.tsv`, `.json`, `.txt` (newline-separated).
- Column-pick prompt for ambiguous 2-column CSV/TSV.
- Output-filename prompt + overwrite/auto-suffix/cancel conflict resolution.
- Unit tests for the `converter` module (pure-logic functions).

**Out of scope:**
- Manual in-TUI word entry (user chose converter only).
- Free-form filesystem path input for sources.
- Automatic switch of `config.word_file` after import (user keeps using existing Word List picker to select it).
- Round-based multiplayer, low-time warning, Windows host investigation (separate v1.2 items, separate PRs).

## File-by-file changes

### New: `crates/client/src/converter.rs`

Pure-logic module. No I/O beyond reading the passed-in `&str` content and emitting a `String`. Everything testable in isolation.

```rust
pub enum SourceFormat { Csv, Tsv, Json, Text }

pub fn detect_format(path: &Path) -> Option<SourceFormat>;

// Categories -> dedup'd word list (case-insensitive dedup, like load_words).
// BTreeMap so output category order is deterministic.
pub type ParsedList = BTreeMap<String, Vec<String>>;

pub enum ConvertError {
    EmptyFile,
    InvalidJson(String),           // serde_json message
    JsonWrongShape,                // not {cat: [strings]}
    TooManyColumns { found: usize },
    EmptyAfterParse,               // parsed OK but zero entries
}

pub enum CsvHeaderAnalysis {
    // word_col resolved (via word-like header match OR single-column file).
    // category_col = Some(idx) iff a 2-col file with a recognized word column.
    AutoResolved { word_col: usize, category_col: Option<usize>, headers: Vec<String> },
    // 2-col file, neither header matches word-like names.
    NeedsPick { headers: Vec<String> },
}

pub fn analyze_csv_headers(content: &str, delimiter: char)
    -> Result<CsvHeaderAnalysis, ConvertError>;

pub fn parse_csv_with_cols(
    content: &str,
    delimiter: char,
    word_col: usize,
    category_col: Option<usize>,
) -> Result<ParsedList, ConvertError>;

pub fn parse_json(content: &str) -> Result<ParsedList, ConvertError>;

pub fn parse_text(content: &str) -> Result<ParsedList, ConvertError>;

// Emits `lists/`-compatible format: "[Category]\nWord1\nWord2\n\n[Next]\n..."
pub fn emit_list(parsed: &ParsedList) -> String;
```

**Word-like header names:** `word`, `name`, `entry`, `term` — matched case-insensitively after trim.

**General category name:** `"General"`.

**Dedup rule:** case-insensitive, applied globally across categories (same as `load_words` in `main.rs`) — first occurrence wins.

**CSV/TSV parsing minutiae:** handwritten splitter, no new crate. Strip surrounding double quotes if both ends quoted. Skip blank lines and header row. Trim each field.

**Tests** (in `#[cfg(test)] mod tests`):
- `detect_format` — by extension, case-insensitive, unknown returns None.
- `parse_text` — trims, drops blanks, dedups case-insensitively, wraps all in `General`.
- `parse_json` — valid map parses; non-object JSON → `JsonWrongShape`; non-string array items → `JsonWrongShape`; malformed → `InvalidJson`.
- `analyze_csv_headers` — single column → `AutoResolved` with `word_col=0, category_col=None`; 2-col with `Word,Category` → `AutoResolved` with `word_col=0, category_col=Some(1)`; 2-col with `Category,Name` → `AutoResolved` with `word_col=1, category_col=Some(0)`; 2-col with `Foo,Bar` → `NeedsPick`; word-like matching is case-insensitive; >2 cols → `TooManyColumns`.
- `parse_csv_with_cols` — explicit column selection works both ways; TSV (delimiter `'\t'`) works; quoted fields are unwrapped; missing category cell falls back to `General`.
- `emit_list` — deterministic category order (BTreeMap), blank line between categories, no trailing blank line except one at EOF.
- "No separator → General" fallback explicitly covered via a single-column CSV *and* a `.txt` file.

### New: `crates/client/src/converter_menu.rs`

The TUI flow for the Import action. Keeping it in its own module avoids bloating `menu.rs` (which is already ~866 lines and slated for a split in v1.4).

```rust
pub async fn run_import_flow(reader: &mut EventStream);
```

Screens, in order:
1. **Source picker** — `ListState`-driven list of files in `imports/`. Empty dir → show an error-like screen with the absolute path and a "Press any key to return" footer. Uses `list_menu::classify_key`.
2. **Column picker** (CSV/TSV only, only when `NeedsPick`) — list of headers; user picks which one is the word column; if 2 cols the other becomes the category column. If 1 col, this screen is skipped.
3. **Output filename prompt** — text-input screen (bespoke, like `run_text_input`). Default = source stem + `.txt`. `.txt` auto-appended if missing. Reject empty, `/`, `\`, `\0`.
4. **Conflict resolution** (only if target exists in `lists/`) — 3-item menu: `Overwrite`, `Use auto-suffix (<name>_N.txt)`, `Cancel`. Auto-suffix picks the smallest integer `N ≥ 1` whose file does not exist.
5. **Result screen** — success message with entry count + category count, or error message. "Press any key to return."

No changes to `config.word_file` — user selects the new list via the existing Word List picker after import, same as if they had manually dropped a `.txt`.

### Modified: `crates/client/src/paths.rs`

Add two helpers, following the `lists_dir` / `list_available_lists` pattern:

```rust
pub fn imports_dir() -> io::Result<PathBuf>;
pub fn ensure_imports_dir() -> io::Result<PathBuf>;    // auto-create on first use
pub fn list_available_imports() -> io::Result<Vec<String>>;  // any extension, sorted
```

`list_available_imports` returns all regular files in `imports/` (not just `.txt`) so the user can see unsupported files too — `detect_format` returning `None` surfaces an error at convert time rather than filtering silently.

### Modified: `crates/client/src/menu.rs`

- Add `Import Word List` entry to the Settings menu between `Color Scheme:` and `Back`. `SETTINGS_COUNT` → 10. The new index 8 is the import action; index 9 is Back.
- Add a new `SettingsResult::OpenImportFlow` variant and dispatch it from both `menu_loop` and `run_settings_inline` to `converter_menu::run_import_flow`.

### Modified: `crates/client/src/main.rs`

- `mod converter;` + `mod converter_menu;`
- No changes to `load_words` / `load_categories` — they already handle the emitted format.

### Modified: `crates/client/src/render.rs`

Add one thin renderer if the existing `render_menu` can't cover the result screen cleanly:

```rust
pub fn render_import_result(msg: &str, success: bool, term_size: (u16, u16));
```

Reuses existing color helpers (`summary_border_colors`, `summary_success_colors`, `error_panel`). For the picker and conflict screens, reuse `render_menu` with `MenuItem` variants.

### Modified: `crates/client/Cargo.toml`

No new dependencies. `serde_json` is already present; the CSV/TSV parser is a short handwritten split-on-delimiter routine.

### Modified: `CLAUDE.md` + `README.md`

- CLAUDE.md: add `imports/` to the install layout blurb; add `converter.rs` and `converter_menu.rs` to the module table; add a bullet under Architecture → Key Implementation Details for the import flow; add a scenario to the Testing section.
- README.md: short "Importing word lists" section describing the drop-in dir + Settings action + supported formats.

## Test plan (manual)

After implementation, before PR:

1. Drop a newline-separated `.txt` into `imports/` → Settings → Import Word List → pick it → accept default name → confirm a new file appears in `lists/` with all words under `[General]`.
2. Drop a 2-column CSV with header `word,category` → convert → confirm categories honored.
3. Drop a 2-column CSV with header `category,name` → confirm auto-detection still picks `name` as the word col.
4. Drop a 2-column CSV with ambiguous headers (`x,y`) → confirm column picker prompts the user.
5. Drop a valid JSON map → convert → confirm category structure preserved.
6. Drop a JSON file with wrong shape → confirm clean error message, no crash.
7. Drop a TSV (tab-separated) → convert → confirm delimiter auto-detected from extension.
8. Import twice with the same output name → confirm overwrite / auto-suffix / cancel options.
9. Pick a file with an unrecognized extension → confirm an unsupported-format error (no crash).
10. Empty `imports/` dir → confirm helpful message with the absolute path.
11. Import → switch to the new list via the existing Word List picker → play a solo game with the new list.
12. `cargo test -p guess_up converter::` → all converter unit tests pass.
13. `cargo build --release` + `cargo clippy` clean.

## Risks / open questions

- **CSV quoting corner cases** — the handwritten parser intentionally handles only the common cases (no embedded commas inside quoted fields with escaped quotes). **Decision: stay with the self-rolled parser, do not pull in the `csv` crate.** Users with gnarly CSVs can export as JSON instead. Revisit only if real users hit it.
- **Very large imports** — no explicit size cap. In practice `lists/` files are small (<1000 lines); not worth engineering a streaming pipeline.
- **Unicode / non-ASCII headers** — header matching uses `.to_ascii_lowercase()` on the trimmed field. Non-ASCII headers won't match `word`/`name`/`entry`/`term` and will fall into the `NeedsPick` path, which is fine.

## Release bookkeeping

Completing these two TODO items does **not** finish all v1.2 items (low-time warning, round-based multiplayer, Windows host bug, list reorg still open). **Do not bump the workspace version** in this PR — the CLAUDE.md rule says the bump goes in the PR that completes the *last* v1.2 item.

Update `TODO.md` by flipping both items to ✅:
- Line 24 — Custom word list support.
- Line 33 — In-TUI word-list converter.

## Commit / PR flow

- Branch name: `custom-word-list-import`.
- One PR, one commit (or a small series if tests land before UI).
- Update README + CLAUDE.md in the same PR (per CLAUDE.md's "Before creating a PR" rule).
