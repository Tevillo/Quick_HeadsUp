# Heads Up CLI Game - Rewrite Plan

## Context

The existing ASOIAF Heads Up game (`exercise-6/`) had a fundamentally broken async architecture: a custom `Future` impl with blocking `stdin().read_line()` inside `poll()`, which blocked the entire tokio runtime. The timer only fired after user input, giving the last question infinite time. The centering logic required pre-padded word list entries (leading spaces on odd-length names). Several features were abandoned mid-implementation (color flash). This rewrite fixes the architecture with channels + `tokio::select!`, adds game modes, and improves the UX.

## Feature Status

### Implemented

- [x] **Channel-based async architecture** -- replaced custom `Future`/`Delay` with `tokio::sync::mpsc` channels + `tokio::select!`
- [x] **Auto-centering** -- `(terminal_width - content_width) / 2`, removed `--even` flag and `make_even()`
- [x] **Single-keypress input** -- crossterm raw mode + `EventStream`, y/n/q with no Enter needed
- [x] **Green/red screen flash** -- non-blocking via `tokio::spawn`, sends `Redraw` event after restore
- [x] **Auto-end on timer** -- `TimerExpired` event breaks game loop immediately
- [x] **`--last-unlimited` flag** -- final question gets unlimited time when timer expires
- [x] **Extra-time mode** -- `--extra-time` + `--bonus-seconds N` flags, bonus channel to timer task
- [x] **Live score + timer** -- updates every second via `TimerTick`, score shown alongside timer
- [x] **Custom word list** -- `--word-file <path>` flag, default `files/ASOIAF_list.txt`
- [x] **Category support** -- `[Category]` headers in word files, `--category <name>` flag to filter
- [x] **Game history** -- saves results to `~/.heads_up_history.json` with serde
- [x] **Terminal bell** -- `\x07` on timer expiry
- [x] **Terminal safety** -- `TerminalGuard` with `Drop` for raw mode + alternate screen cleanup
- [x] **Fisher-Yates shuffle** -- O(n) word selection, graceful exhaustion handling
- [x] **Box-drawn countdown** -- replaced `rascii_art` PNG rendering with centered block-style digits
- [x] **End-of-round summary** -- boxed output showing score, accuracy %, pace, and missed words
- [x] **Word list cleanup** -- trimmed whitespace, removed duplicates, organized into 25 categories
- [x] **Module split** -- 6 modules: `main.rs`, `types.rs`, `game.rs`, `input.rs`, `timer.rs`, `render.rs`

### Not Yet Implemented

- [ ] **Networked mode** -- two machines connected, one sees timer/scores, other sees names (long-term goal)
- [ ] **Game history CLI viewer** -- no way to view `~/.heads_up_history.json` from the CLI yet
- [ ] **Configurable key bindings** -- keys are hardcoded (y/n/q)
- [ ] **Multi-round / replay** -- game exits after one round, no "play again?" prompt
- [ ] **Sound effects beyond bell** -- only terminal bell on expiry, no per-answer sounds
- [ ] **README update** -- README still reflects v0.1 usage

## Architecture: Channel-Based Async

Three concurrent tasks communicating via `tokio::sync::mpsc` channels:

```
  Timer Task ----> tx ----+
                          v
  Input Task ----> tx --> Game Loop (tokio::select!) ----> Render functions
```

- **Input task** (`src/input.rs`): Uses `crossterm::event::EventStream` in raw mode. Single keypress registers instantly. Sends `GameEvent::UserInput(action)`.
- **Timer task** (`src/timer.rs`): Ticks every second, sends `GameEvent::TimerTick(remaining)`. Sends `GameEvent::TimerExpired` at zero. In extra-time mode, receives bonus seconds on a separate `BonusReceiver` channel.
- **Game loop** (`src/game.rs`): `tokio::select!` on the event receiver. Handles input (score, flash, next word), ticks (re-render timer), expiry (end game or enter last-unlimited state), and redraws (after flash restores colors).

## Module Structure

```
src/
  main.rs    -- CLI args (clap), word loading with category parsing, channel setup, task spawning
  types.rs   -- GameEvent, UserAction, GameMode, GameConfig, GameResult, GameSummary, type aliases
  game.rs    -- GameState, GameSummary, game loop (select!), Fisher-Yates shuffle, history saving
  input.rs   -- input_task() using crossterm EventStream + raw mode
  timer.rs   -- timer_task() with 1s interval ticks and bonus-time channel
  render.rs  -- TerminalGuard, render_question(), render_question_unlimited(), render_countdown(),
                flash_screen(), print_output(), bell(), centering math
```

## Key Implementation Details

### Countdown (render.rs)
Originally used `rascii_art` to render PNG images as ASCII art. This was replaced with hardcoded box-drawing block digits (`██████╗` style) that use the same `center_col()` centering as game questions. Works consistently in any terminal size including split panes. Removed `rascii_art` dependency and the three `*_skinny.png` files.

### End-of-round summary (render.rs, game.rs)
`run_game()` returns a `GameSummary` struct. `main()` explicitly drops the `TerminalGuard` (leaving alternate screen and raw mode) *before* calling `print_output()`. This was a bug fix -- previously the summary printed inside the alternate screen buffer and was wiped when the guard dropped. The summary box shows:
- Score (correct / total)
- Correct vs Passed breakdown
- Accuracy percentage
- Pace (answers per minute)
- Missed words (wrapped to fit the box)
- "You cleared the entire list!" if all words answered

### Flash race condition handling
`flash_screen()` clears the entire terminal to show the color, which clobbers the current question display. After the 150ms flash, it sends a `GameEvent::Redraw` event so the game loop re-renders the current word. This prevents a blank screen between flash and next input.

### Word loading (main.rs)
`load_words()` parses `[Category]` headers, trims all lines, deduplicates via `HashSet` (case-insensitive), and filters by `--category` if provided. The word list file was cleaned: all leading/trailing whitespace removed, organized into 25 categories (House Stark, Dragons, Valyrian Steel, Theories, etc.).

## CLI Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `-g, --game-time` | u64 | 60 | Game length in seconds |
| `-s, --skip-countdown` | bool | false | Skip the 3-2-1 countdown animation |
| `-l, --last-unlimited` | bool | false | Give unlimited time on the last question |
| `-x, --extra-time` | bool | false | Enable extra-time mode |
| `--bonus-seconds` | u64 | 5 | Seconds added per correct answer (extra-time mode) |
| `-w, --word-file` | String | files/ASOIAF_list.txt | Path to word list file |
| `--category` | Option\<String\> | None | Filter to a specific category in the word file |

Removed from v0.1: `--countdown` (renamed to `--skip-countdown`, inverted logic), `--even` (no longer needed)

## Dependencies (Cargo.toml)

```toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
crossterm = { version = "0.27", features = ["event-stream"] }
rand = "0.8"
tokio = { version = "1.39", features = ["macros", "rt-multi-thread", "time", "sync"] }
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
dirs = "5"
```

Changes from v0.1:
- `crossterm`: added `event-stream` feature for async key events
- `tokio`: added `time` (sleep, intervals) and `sync` (mpsc channels) features
- Added `futures` for `StreamExt::next()` on `EventStream`
- Added `serde` + `serde_json` for game history JSON serialization
- Added `chrono` for timestamps in history entries
- Added `dirs` for `home_dir()` to locate `~/.heads_up_history.json`
- Removed `rascii_art` (countdown rewritten with box-drawing characters)
- Removed `term_size` (crossterm provides `terminal::size()`)

## Long-term: Networked Mode

Not implemented yet. The channel architecture is designed to enable this:
- Replace `input_task` with a network listener (TCP/WebSocket)
- "Holder" machine runs input + sends `UserAction` over the network
- "Viewer" machine runs timer + render + receives actions from network
- Add `GameMode::Networked { role }` variant to `types.rs`

The module split (`input.rs` separate from `game.rs`) specifically enables swapping the input source without touching game logic.

## Verification

1. `cargo build` -- compiles without warnings
2. `cargo run` -- normal mode: words display centered, single-keypress works, timer counts down live, flash on y/n, auto-ends on expiry, summary prints after
3. `cargo run -- --last-unlimited` -- timer expires, shows "LAST QUESTION", waits for answer, then ends
4. `cargo run -- --extra-time --bonus-seconds 3` -- correct answers add 3 seconds to timer
5. `cargo run -- --word-file files/ASOIAF_list.txt --category "House Stark"` -- only Stark names appear
6. Check `~/.heads_up_history.json` exists after a game
7. Ctrl+C during game -- terminal restores cleanly (raw mode off, alternate screen exited)
8. Run with very long names and very short names -- centering is correct for both
