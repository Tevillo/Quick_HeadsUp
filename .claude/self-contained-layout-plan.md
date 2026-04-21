# Self-Contained Install Layout

Ship `guess_up` so the binary runs from its own directory with two siblings:

- `./.history/` — game history (replaces `~/.guess_up_history.json`)
- `./lists/` — word list files (replaces hardcoded `files/ASOIAF_list.txt`)

Config stays at `~/.guess_up_config.json` (explicitly out of scope).

## Scope Decisions (from planning discussion)

1. **Dev paths** — always resolve via `std::env::current_exe()?.parent()`. No special-casing for `cargo run`; dev just means the binary lives at `target/{debug,release}/guess_up` and looks for siblings there. A `build.rs` copies repo lists into `target/{profile}/lists/` at build time so `cargo run` works out of the box.
2. **`lists/` structure** — multiple `.txt` files, one themed list each. Menu adds a "Word List" picker that scans `lists/*.txt`. `AppConfig::word_file` becomes a **filename** relative to `lists/` (e.g. `"ASOIAF_list.txt"`), not a full path.
3. **History layout** — single `.history/history.json` (same JSON array format as today). `.history/` auto-created when missing.
4. **Missing dirs** — `.history/` auto-creates silently. Missing or empty `lists/` produces a clear on-screen error; app exits gracefully. **No migration** of the old `~/.guess_up_history.json`.

## Branch

`self-contained-layout` — feature branch, merge via PR.

---

## File Changes

### New: `crates/client/src/paths.rs`

Central path resolution. All callers go through this — never reach for `current_exe()` or `dirs::home_dir()` for install-layout paths anywhere else.

```rust
use std::fs;
use std::io;
use std::path::PathBuf;

pub fn install_dir() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "executable has no parent dir"))
}

pub fn history_dir() -> io::Result<PathBuf> { Ok(install_dir()?.join(".history")) }
pub fn history_file() -> io::Result<PathBuf> { Ok(history_dir()?.join("history.json")) }
pub fn lists_dir() -> io::Result<PathBuf> { Ok(install_dir()?.join("lists")) }

pub fn ensure_history_dir() -> io::Result<PathBuf> {
    let dir = history_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns sorted `.txt` filenames (not full paths) in `lists/`.
/// Err if `lists/` is missing. Ok(empty vec) if it exists but has no `.txt` files —
/// caller decides how to surface that.
pub fn list_available_lists() -> io::Result<Vec<String>> {
    let dir = lists_dir()?;
    let entries = fs::read_dir(&dir)?;
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("txt") {
                p.file_name().and_then(|s| s.to_str()).map(String::from)
            } else {
                None
            }
        })
        .collect();
    out.sort();
    Ok(out)
}

pub fn word_file_path(filename: &str) -> io::Result<PathBuf> {
    Ok(lists_dir()?.join(filename))
}
```

Register in `main.rs`: `mod paths;`

### `crates/client/src/config.rs`

- Change default `word_file`: `"files/ASOIAF_list.txt"` → `"ASOIAF_list.txt"` (filename only).
- Leave load/save of `~/.guess_up_config.json` alone.
- (No validation here — `main.rs` resolves + validates on startup.)

### `crates/client/src/main.rs`

- Add `mod paths;`.
- Change `load_words` and `load_categories` signatures from `path: &str` to `filename: &str`; resolve via `paths::word_file_path(filename)?`. Return `io::Result<Vec<String>>` so callers can handle missing files cleanly (or keep `Vec<String>` and log + return empty — existing behavior is lenient; match current style).
- `main()`:
  - Before entering `menu_loop`, call `paths::list_available_lists()`. 
    - `Err(_)` (lists/ missing): render an error screen ("No `lists/` directory found at {path}. Create it and add at least one `.txt` word list.") and exit.
    - `Ok(empty)`: render an error screen ("No word lists found in {path}. Add at least one `.txt` file.") and exit.
    - `Ok(names)`: if `config.word_file` is not in `names`, set `config.word_file = names[0].clone()` and save; pass through to `menu_loop`.
- Update `run_solo` call site at line 76 (already passes `&app_config.word_file`, which is now a filename — `load_words` does the resolution).

### `crates/client/src/lobby.rs`

- Line 55: same story — `load_words` now takes a filename, no call-site change needed beyond making sure we still pass `&app_config.word_file`.

### `crates/client/src/game.rs` (`save_history`, lines 441–472)

Replace:
```rust
let Some(home) = dirs::home_dir() else { return; };
let path = home.join(".guess_up_history.json");
```
with:
```rust
let Ok(_) = crate::paths::ensure_history_dir() else { return; };
let Ok(path) = crate::paths::history_file() else { return; };
```
Rest of the function stays the same.

### `crates/client/src/menu.rs`

- Add a new settings entry: **Word List** (filename picker), positioned above the existing Category entry.
- When selected → `SettingsResult::OpenWordListPicker` → runs a new `run_word_list_picker` that calls `paths::list_available_lists()` and shows a scrolling list (reuse the category picker's rendering pattern).
- Selecting a new list:
  - Sets `config.word_file = chosen_filename`.
  - Resets `config.category = None` (old category may not exist in the new list).
- The existing "Word File:" text-input row (line 318–322) should be removed — users now pick from the discovered list instead of typing a path.

### `crates/client/src/render.rs`

- Add `render_word_list_picker(...)` analogous to `render_category_picker`. It's OK to reuse the category picker renderer verbatim if the shape is identical (list of strings + selected index + scroll offset); consider consolidating into a generic `render_list_picker` if both end up identical.

### New: `crates/client/build.rs`

Copies the canonical repo word lists into `target/{profile}/lists/` so `cargo run` works against the same layout as a release install.

```rust
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    // OUT_DIR is target/{profile}/build/{crate}-{hash}/out
    // Walk up to target/{profile}/
    let target_profile_dir: PathBuf = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("target profile dir")
        .to_path_buf();

    let dest = target_profile_dir.join("lists");
    fs::create_dir_all(&dest).expect("create lists/");

    // Repo root is two up from this crate (crates/client/)
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lists");
    println!("cargo:rerun-if-changed={}", src.display());

    if let Ok(entries) = fs::read_dir(&src) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("txt") {
                let name = p.file_name().expect("filename");
                fs::copy(&p, dest.join(name)).expect("copy list file");
            }
        }
    }
}
```

Update `crates/client/Cargo.toml` to ensure `build = "build.rs"` is set (Cargo default, usually fine).

### Repo layout: rename `files/` → `lists/`

Rename the directory at the repo root so the canonical source matches the install-time layout. `files/ASOIAF_list.txt` becomes `lists/ASOIAF_list.txt`. `build.rs` above reads from that.

### Docs

- `README.md`:
  - Replace references to `files/ASOIAF_list.txt` with `lists/ASOIAF_list.txt`.
  - Replace `~/.guess_up_history.json` with `./.history/history.json` (relative to binary).
  - Add a short "Install layout" section: here's the binary, here's `lists/` next to it, `.history/` is created on first run.
  - Keep the `~/.guess_up_config.json` mention — config is unchanged.
- `CLAUDE.md`:
  - Update the "History" and word-list paths in both the architecture table and the Testing section.
  - Add a sentence to the architecture overview noting the install-directory layout and the `paths.rs` module.

### `.gitignore`

- Add `/target/` stays as-is.
- (No new ignores needed — `.history/` only appears in `target/{profile}/` which is already ignored.)

---

## Manual Test Plan

1. `cargo build` — no warnings. `target/debug/lists/ASOIAF_list.txt` exists after build.
2. `cargo run -p guess_up` — menu renders. Play a solo game. `target/debug/.history/history.json` is created with the result.
3. Delete `target/debug/lists/` → `cargo run -p guess_up` shows the "no lists/ directory" error and exits.
4. Empty `target/debug/lists/` (delete the `.txt`) → startup shows the "no word lists" error and exits.
5. Drop a second list file (e.g. `lists/tolkien.txt`) at the repo root, rebuild → menu settings → Word List picker shows both files. Select one → category resets, starting a game uses the chosen list.
6. `cargo build --release && ./target/release/guess_up` — binary runs against `target/release/lists/`. `.history/` appears there after a game.
7. Copy `target/release/guess_up` + `target/release/lists/` to some other directory, run from there — plays fine, creates `.history/` in that directory.
8. Networked host/join still works end-to-end (no path code in the relay; client changes only).

## Open Follow-ups (not in this PR)

- The existing TODO item "Custom word list support" becomes trivial after this: users drop a `.txt` into `lists/` and it shows up in the picker. Can likely be closed once this lands, or narrowed to "validate custom list format on import."
- The TODO "Game history CLI viewer" will need its path reference updated.

## Final Step After Plan Approval

The user may want to `/clear` context and start a fresh session keyed off this plan file before implementing.
