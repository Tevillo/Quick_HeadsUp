# Plan: Replace Clap with TUI Menu System

## Context

The client currently uses clap to parse CLI args for all game settings and network config. The goal is to remove clap entirely and replace it with an interactive TUI menu system — arrow/hjkl navigation, Enter to select — so players configure everything inside the game. Settings persist between sessions. The relay server keeps its clap args unchanged.

## New Files

| File | Purpose |
|------|---------|
| `crates/client/src/config.rs` | `AppConfig` struct, defaults, load/save `~/.guess_up_config.json`, `to_game_config()` |
| `crates/client/src/menu.rs` | Menu state machine, rendering, input handling, `menu_loop()` → `MenuAction` |

## Modified Files

| File | Change |
|------|--------|
| `main.rs` | Remove `Args`/`Command`/clap imports. Add `mod config; mod menu;`. Rewrite `main()` to: load config → menu loop → dispatch. Change `run_solo`/`run_host`/`run_join` to take `AppConfig`. Wrap in loop for return-to-menu. |
| `lobby.rs` | Replace `&Args` param with `&AppConfig` in `run_host_session` and `run_joiner_session`. Update `load_words` call on line 114. |
| `render.rs` | Add `MenuItem` enum, `render_menu()`, `render_category_picker()` (~100-120 new lines). Existing functions unchanged. |
| `Cargo.toml` | Remove `clap` dependency. |
| No changes to: `types.rs`, `game.rs`, `input.rs`, `timer.rs`, `net.rs` |

## Config File (`~/.guess_up_config.json`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub game_time: u64,           // 60
    pub skip_countdown: bool,     // false
    pub last_unlimited: bool,     // false
    pub extra_time: bool,         // false
    pub bonus_seconds: u64,       // 5
    pub word_file: String,        // "files/ASOIAF_list.txt"
    pub category: Option<String>, // None = All
    pub recent_servers: Vec<String>, // most-recent-first, cap 10
}
```

Uses `#[serde(default)]` so new fields don't break old config files. Load falls back to `Default` if file missing/malformed. Save on every menu exit.

## Menu Screens

```
MenuScreen::Main           →  Solo / Host Game / Join Game / Settings / Quit
MenuScreen::Settings       →  All game settings (toggle/increment/text input)
MenuScreen::CategoryPicker →  Scrollable list: "All" + 32 categories from word file
MenuScreen::ServerConnect  →  Text input for relay address + recent servers list below
MenuScreen::JoinRoom       →  Text input for room code (after server selected)
```

**Navigation:** Up/k, Down/j move selection. Enter selects/confirms. Esc/q goes back. Left/h and Right/l adjust numeric values or toggle booleans in settings.

**Text input mode:** Enter on a text field activates edit mode. Printable chars append, Backspace deletes, Enter confirms, Esc cancels.

**Settings editing:**
- Booleans: Enter or Left/Right toggles. Display as `[x]` / `[ ]`
- Numbers: Left/Right increment/decrement by step. Enter opens text input for exact value
- Category: Enter opens CategoryPicker sub-screen
- Word file: Enter opens text input

**Return type:**
```rust
pub enum MenuAction {
    Solo,
    Host { relay_addr: String },
    Join { relay_addr: String, room_code: String },
    Quit,
}
```

## Rendering Additions to `render.rs`

Add `MenuItem` enum for row types:
```rust
pub enum MenuItem<'a> {
    Label(&'a str),                                    // non-selectable (title, blank)
    Action(&'a str),                                   // selectable item
    Setting { label: &'a str, value: &'a str },        // "Game Time:    60"
    TextInput { label: &'a str, value: &'a str, editing: bool }, // editable field
}
```

Add `render_menu(title, items, selected, selectable_indices, term_size)` — centered box, highlighted row gets `DarkYellow` on `Blue`, others `White` on `Blue`. Reuses the same `queue!/print!/flush` pattern as existing rendering.

Add `render_category_picker(categories, selected, scroll_offset, term_size)` — windowed list (~15 visible at a time) with scroll indicators.

## New Flow in `main()`

```rust
#[tokio::main]
async fn main() {
    let mut config = config::AppConfig::load();

    loop {
        let _guard = render::TerminalGuard::new();
        let action = menu::menu_loop(&mut config).await;
        drop(_guard);

        config.save();

        match action {
            MenuAction::Solo => run_solo(&config).await,
            MenuAction::Host { relay_addr } => run_host(&config, relay_addr).await,
            MenuAction::Join { relay_addr, room_code } => run_join(&config, relay_addr, room_code).await,
            MenuAction::Quit => break,
        }
    }
}
```

Game functions return after the game ends → loop back to menu.

## Helper: `load_categories()`

Add alongside `load_words` in `main.rs`:
```rust
pub fn load_categories(path: &str) -> Vec<String> { ... }
```
Reads `[Category]` headers from the word file. Called by the menu when entering the category picker.

## Implementation Phases

**Phase 1 — Config module:** Create `config.rs` with AppConfig, Default, load/save, to_game_config.

**Phase 2 — Wire config in:** Change run_solo/run_host/run_join to accept AppConfig instead of Args. Update lobby.rs. Temporarily construct AppConfig from Args so game still works.

**Phase 3 — Rendering:** Add MenuItem, render_menu, render_category_picker to render.rs.

**Phase 4 — Menu module:** Build menu.rs with all screens — main menu, settings, category picker, server connect, join room.

**Phase 5 — Integration:** Replace main() with menu loop. Remove Args/Command/clap. Remove clap from Cargo.toml. Add return-to-menu loop.

**Phase 6 — Polish:** Handle terminal resize during menus. Add title banner.

## Verification

1. `cargo build -p guess_up` — compiles without clap
2. Run game → main menu renders, arrow/hjkl navigation works
3. Settings → change game_time → exit → re-run → value persisted
4. Solo → plays full game → returns to main menu
5. Host → server connect screen → type address → connect → room created
6. Join → server connect screen → select recent server → enter room code → join works
7. Category picker → scroll through 32 categories → select one → only those words appear
8. Esc from any sub-menu returns to parent
9. Quit from main menu exits cleanly, terminal restored
