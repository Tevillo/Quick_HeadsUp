# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

An ASOIAF (A Song of Ice and Fire) themed "Guess Up" party game for the terminal, built in Rust. Players see a name/term on screen and press `y` (correct) or `n` (pass) before the timer runs out. Supports solo play and networked multi-player mode (1 host + up to 8 joiners) via a relay server.

## Install Layout

The binary is self-contained: `guess_up` resolves its data dirs from `current_exe()?.parent()` via `crates/client/src/paths.rs` (the single source of truth — never reach for `current_exe()` or `dirs::home_dir()` for install-layout paths elsewhere). Two siblings live next to the binary:

- `./lists/` — word list `.txt` files (required; startup fails with an on-screen error if missing or empty)
- `./.history/history.json` — game history (created on first game)
- `./imports/` — drop-in directory for source files (`.csv`, `.tsv`, `.json`, `.txt`) consumed by the Settings → Import Word List flow (auto-created on first use; visible, not hidden)

User config `.guess_up_config.json` lives next to the binary — resolved by `paths::config_path()` (same source-of-truth pattern as `lists/`, `.history/`). `AppConfig::word_file` stores a **filename** (e.g. `"ASOIAF_list.txt"`), not a path — resolution happens through `paths::word_file_path`. A `build.rs` copies the repo's `lists/` into `target/{profile}/` so `cargo run` works against the same layout as a release install; during development each `target/{debug,release}/` profile therefore has its own independent config file.

## Build & Run

```bash
cargo build --release                          # build all crates
cargo run -p guess_up                          # launches TUI menu

# Relay server
cargo run -p relay                             # binds to 0.0.0.0:3000
```

All game settings (game time, categories, extra-time mode, relay server, etc.) are configured through the interactive TUI menu. Settings persist between sessions in `.guess_up_config.json` next to the binary. No CLI flags needed for the client.

## Coding Standards

- **Formatting**: `cargo fmt` with default settings.
- **Linting**: `cargo clippy` — all warnings should be clean.
- **Error handling**: Prefer `Result` propagation with `?` over `.expect()` / `.unwrap()`. Existing `.expect()` calls are legacy and should migrate toward `Result`-based handling over time.
- **Style**: Idiomatic Rust — `match` expressions, iterators, `Option`/`Result` types. Snake_case functions, PascalCase types.

## Architecture

Cargo workspace with three crates:

```
crates/
  protocol/   # shared message types (ClientMessage, RelayMessage, GameMessage) + TCP framing (length-prefixed JSON)
  relay/      # standalone relay server binary — room management (1 host + up to 8 peers), message forwarding (broadcast/targeted), heartbeat pings
  client/     # the game binary (solo + networked modes)
```

### Client Architecture

Channel-based async with tokio. Concurrent tasks communicate via `tokio::sync::mpsc`:

```
Input Task (crossterm EventStream) ---> tx --+
                                             v
Timer Task (1s interval ticks)     ---> tx --> Game Loop (tokio::select!) --> Render
                                             ^
Network Task (TCP via relay)       ---> tx --+  (networked mode only)
```

### Client Modules

| Module | Responsibility |
|--------|---------------|
| `main.rs` | Word loading with category parsing, startup validation of `lists/`, game runners (solo/host/join), entry point |
| `config.rs` | `AppConfig` struct, defaults, load/save `.guess_up_config.json` (next to the binary via `paths::config_path`), `to_game_config()`. `word_file` is a filename within `lists/`, not a full path. `color_scheme` is a scheme id (e.g. `"classic"`, `"stark"`) resolved through `theme::by_id`; default is `"stark"`, and `theme::default_scheme()` matches. `auto_rotate_holder` is a host-mode flag (defaults true) that replaces the post-game holder picker with automatic rotation + session-total tracking; users can opt into the manual Pick Next Holder flow via Settings. |
| `theme.rs` | Color scheme table (12 truecolor palettes — 3 generic + 9 ASOIAF great houses) and the active-scheme `OnceLock<RwLock<&'static ColorScheme>>`. `render.rs` resolves every `SetColors` call through helpers (`fg_on_primary`, `accent_on_primary`, `accent_on_selection`, `error_panel`, `summary_border_colors`, `summary_accent_colors`, `summary_success_colors`) against the active scheme. The color scheme picker (`render_color_scheme_picker`) shows a live preview panel to the right of the list — each hovered scheme is rendered as sample bars (menu text, selected item, title, summary, error) in its own palette while the menu itself stays in the currently-active scheme. Enter commits the hovered scheme via `theme::set_active`; Esc cancels without mutating active state. Flash correct/pass stay hardcoded green/red across all schemes. Requires truecolor (24-bit RGB) terminal. |
| `paths.rs` | Single source of truth for install-layout paths — `install_dir`, `lists_dir`, `imports_dir`, `history_dir`, `config_path`, `word_file_path`, `list_available_lists`, `ensure_history_dir`, `ensure_imports_dir`, `list_available_imports`, plus `mark_hidden` (no-op on non-Windows) used to set the hidden attribute on `.history/` and the config file on first create. |
| `menu.rs` | TUI menu state machine (`menu_loop`), all screens (main, settings, word list picker, category picker, server connect, join room), game dispatch. Settings carries an `Import Word List` action that hands off to `converter_menu::run_import_flow`. |
| `converter.rs` | Pure-logic word-list converter — parses CSV/TSV/JSON/plain text into a category → words `BTreeMap`, emits a `lists/`-compatible string. No I/O; everything is deterministic and unit-tested. `detect_format` picks a parser by extension, `analyze_csv_headers` classifies 1-col vs 2-col-auto vs 2-col-ambiguous, and `emit_list` produces deterministic output (alphabetical category order, blank line between categories, single trailing newline). Dedup is case-insensitive across the whole file, first-occurrence-wins — matches `load_words`. |
| `converter_menu.rs` | TUI flow for Settings → Import Word List. Screens: source picker (files in `imports/`), optional two-step column picker for CSV/TSV when headers aren't auto-resolvable — word-column picker followed by category-column picker (the latter includes a "None — put everything under [General]" option), output filename prompt (default = source stem + `.txt`), conflict resolution (overwrite / auto-suffix / cancel when target exists in `lists/`), result screen via `render::render_import_result`. Writes into `lists/` but never mutates `config.word_file` — the user selects the new list through the existing Word List picker. |
| `list_menu.rs` | Shared helpers for list-style screens — `ListState` (selected cursor + scroll offset with wraparound `on_up`/`on_down` and `ensure_visible`) and `classify_key` (maps Up/k, Down/j, Enter, Esc/q to a `ListKey` enum). Used by main menu, category picker, word list picker, holder picker, and the post-game menu's Esc/q → Quit path. Text-input screens (server connect, join room) and the color-scheme picker keep their bespoke handlers. |
| `types.rs` | `GameEvent`, `UserAction`, `GameMode`, `GameConfig`, `GameResult`, type aliases |
| `game.rs` | `GameState`, game loop (`tokio::select!`), Fisher-Yates shuffle, history saving. `run_game` is the host/solo loop, `run_remote_game` is the joiner's display-only loop |
| `input.rs` | `input_task()` — crossterm `EventStream`, single-keypress in raw mode |
| `timer.rs` | `timer_task()` — 1s interval ticks, bonus-time channel for extra-time mode. `blink_task()` — 500ms (2Hz) interval emitting `GameEvent::BlinkTick` for the low-time warning blink; spawned alongside the timer in solo, host, and joiner game loops. `WARNING_THRESHOLD_SECS = 10` is the shared threshold below which the warning kicks in. |
| `render.rs` | `TerminalGuard` (RAII cleanup), `MenuItem` enum, menu rendering, game rendering, flash, countdown, lobby screens, `render_game_summary` (in-TUI end-of-game stats box with caller-supplied action hints), `render_import_result` (success/error screen for the word-list import flow) |
| `net.rs` | `NetConnection` (TCP connect + handshake, split/reassemble), `ConnectError` (IO / InvalidMagic / VersionMismatch / HandshakeEof), `NetHandle` (spawn read/write tasks, recoverable shutdown), `OutboundMsg` (Broadcast/SendTo routing) |
| `lobby.rs` | Room creation, host lobby (wait for players with live participant list), holder selection (pick any participant), joiner session loop, post-game menu (play again / pick next holder / quit), connection recovery across games |
| `terminal_spawn.rs` | Detect missing TTY (`IsTerminal` on stdin+stdout) and re-launch the binary inside a terminal emulator. Linux picker: `$TERMINAL` → `xdg-terminal-exec` → built-in fallback list. Windows picker: `wt.exe` → `cmd.exe /c start`. Loop-safe via `GUESS_UP_SPAWNED=1` sentinel. Opt-out with `--no-spawn-terminal`. On total failure, appends a timestamped entry to `<install_dir>/.guess_up_launch_error.log` and exits 1. |

The key invariant is that `input.rs` stays separate from `game.rs` to allow swapping input sources.

### Key Implementation Details

- **TerminalGuard**: RAII pattern with `Drop` — ensures raw mode and alternate screen are cleaned up on panic or early exit.
- **Self-spawn sentinel**: `terminal_spawn::spawn_if_needed` is the first thing `main()` runs. It skips when `--no-spawn-terminal` is passed, when `GUESS_UP_SPAWNED=1` is set (sentinel written on the child's env so we can't fork-bomb), or when either stdin/stdout is already a TTY. Only fires on full detachment (file-manager launch, detached systemd unit, etc.).
- **Flash race condition**: `flash_screen()` clobbers the display; after 150ms it sends `GameEvent::Redraw` so the game loop re-renders the current word. Both game loops track a `flashing` flag to skip renders while the flash is on screen, preventing the game loop or timer ticks from overwriting the flash effect.
- **Summary rendering**: `render::render_game_summary` draws the end-of-game stats *inside* the alt screen — solo, host, and joiner all keep their `TerminalGuard` alive. Callers pass a list of action hints (`"[P] Play again"`, `"Press any key to continue..."`, `"Waiting for host..."`) that render below the stats inside the same box. Missed words wrap across up to 3 lines and are truncated with `"...and N more"` when the list is longer.
- **Connection recovery**: In networked mode, `NetHandle::shutdown()` recovers the TCP reader/writer from background tasks so the connection can be reused across games without reconnecting. Both host and joiner recover connections after each game for multi-round play.
- **Host-authoritative model**: The host owns all game state (words, timer, score). Joiners run `run_remote_game` which only renders based on messages received from the host. Input routing depends on role — Viewer processes `RemoteInput` from the holder's `PeerId`, Holder processes `UserInput`. The host picks who the Holder is from a participant list (can be themselves or any joiner).
- **Auto-rotate Holder** (`AppConfig::auto_rotate_holder`, default false): host-mode toggle. When on, `run_host_session` starts with the host as initial Holder (no picker), and after each round advances to the next participant in join order via `next_holder_in_rotation` (host → peer1 → peer2 → … → host). The post-game menu is replaced with a two-choice prompt (Enter = next round, Q = quit), and a running `(session_score, session_total)` tally is passed into `render::render_game_summary` via its optional `session_tally` argument — rendered as a "Session: X / Y" success row above the missed-words list. Joiners see only the round summary; the session tally is host-side only.
- **Low-time warning**: during the last `timer::WARNING_THRESHOLD_SECS` (10) seconds of a round, `render_question` and `render_holder_view` color the timer line red and, when `WarningState::border_red` is on, draw a red outline around the outer edge of the terminal. A 500ms blink task (`timer::blink_task`) toggles `border_red` via `GameEvent::BlinkTick` for a 2Hz flash; the game loop only re-renders on `BlinkTick` when already inside the warning window. Joiners run their own local blink task — host-side `TimerSync` is still only 1Hz, so the blink is driven purely by the joiner's local clock. Flashing suppresses warning redraws the same way it suppresses regular ticks.
- **Multi-viewer rooms**: Rooms support 1 host + up to 8 joiners. The relay server manages a `Vec<Peer>` per room, assigns monotonically increasing PeerIds, and routes messages (broadcast or targeted). Only host disconnect removes the room; joiner disconnect is non-fatal.
- **Room codes**: Picked from a hardcoded ASOIAF pool in `crates/relay/src/room_codes.rs` (single-word alphabetic entries ≤8 chars, stored uppercase). `RelayServer::pick_code` tries the pool up to `MAX_POOL_ATTEMPTS` (8) times on collision before falling back to the legacy 5-char random A-Z generator. Joins are case-insensitive — incoming `JoinRoom { code }` is uppercased at the protocol boundary in `handle_connection`.
- **Joiner post-game**: After a game, the joiner waits for the host's next `RoleAssignment` (signaling a new round) rather than intermediate `PlayAgain`/`PickNextHolder` messages. This avoids a race condition where the net read task could consume messages during shutdown.
- **Menu-driven game dispatch**: `menu_loop` owns the full lifecycle — it runs games internally and loops back to the appropriate screen (server connect, room code) after each game ends, preserving menu state.
- **Settings persistence**: `AppConfig` is loaded from `.guess_up_config.json` (next to the binary) on startup and saved after each menu exit or game. `#[serde(default)]` ensures forward compatibility. On Windows the file is marked hidden on first create.
- **Address validation**: Relay server addresses are validated and normalized by `menu::normalize_address` before connection attempts (numeric port 1-65535). If the user omits the port — no colon, or trailing colon with nothing after — the default `DEFAULT_RELAY_PORT` (3000) is appended. Errors display inline in red on the input screen.
- **Word loading**: Parses `[Category]` headers, trims lines, deduplicates via `HashSet` (case-insensitive).
- **History**: Saved to `./.history/history.json` (next to the binary) via serde. The directory is auto-created on first save.
- **Word-list import**: Users drop source files into `imports/` (auto-created on first use) and open Settings → Import Word List. `converter::detect_format` picks a parser by extension (`.csv`, `.tsv`, `.json`, `.txt`); anything else yields an unsupported-format error at convert time — `paths::list_available_imports` intentionally surfaces every file regardless of extension so unsupported drops are visible. CSV/TSV auto-detects the word column when a header matches `word`/`name`/`entry`/`term` (case-insensitive) in a 2-column file. Any file where the layout isn't auto-resolvable (2-column with unrecognized headers, or 3+ columns) routes to a two-step picker: word column first, then category column. The category picker always offers a "None — put everything under [General]" option so files without a category dimension still round-trip cleanly. JSON must be `{ "Category": ["word", ...], ... }` — any other shape is rejected cleanly. Output goes into `lists/`; on target collision the user picks overwrite / auto-suffix (`<name>_N.txt`) / cancel. `config.word_file` is never mutated by the import — the user selects the new list via the existing Word List picker.

### Protocol

The `protocol` crate defines length-prefixed JSON framing over TCP (`read_frame`/`write_frame`, max 64KB). Peers are identified by `PeerId` (u8); host is always `HOST_PEER_ID` (0), joiners get 1, 2, 3, etc. assigned by the relay.

**Handshake preamble** (v1.1+): the first frame on every connection is a client-sent `Handshake { magic, version }`. The relay validates both fields before reading any `ClientMessage`:
- `magic` must equal `HANDSHAKE_MAGIC` (`"GUESSUP"`) — otherwise the relay replies `HandshakeResponse::InvalidMagic` and closes.
- `version` must exactly equal the relay's `CARGO_PKG_VERSION` — otherwise the relay replies `HandshakeResponse::VersionMismatch { relay_version }` and closes.
- On success the relay replies `HandshakeResponse::Ok` and the normal `ClientMessage` flow begins.

Client and relay must be built from the same workspace version; the handshake is a protocol-sanity check, not access control. The client surfaces rejection errors inline (e.g. `"version mismatch: client 1.1.0, relay 1.0.0"`).

**Message layers:**
- **ClientMessage**: client → relay (CreateRoom, JoinRoom, GameData { msg, target }, Disconnect, Pong). `target: None` broadcasts, `target: Some(id)` sends to a specific peer.
- **RelayMessage**: relay → client (RoomCreated, PeerJoined { peer_id }, JoinedRoom { peer_id }, PeerList { peers }, GameData { msg, from }, PeerDisconnected { peer_id }, Ping)
- **GameMessage**: peer ↔ peer (forwarded through relay) — RoleAssignment { holder_id }, word updates, timer sync, score, input, post-game actions (PlayAgain, PickNextHolder, QuitSession). Auto-rotate mode reuses `PlayAgain` for the "next round" signal since the joiner only cares that a new round is starting — the upcoming `RoleAssignment` tells them who the new Holder is.

## Testing

Manual play-testing is the primary test strategy. Key scenarios to verify:

1. `cargo run -p guess_up` — main menu renders, arrow/hjkl navigation works
2. Settings → change game_time → exit → re-run → value persisted in `.guess_up_config.json` next to the binary
3. Solo → plays full game → end-of-game stats render inside the alt screen → any key returns to main menu
4. Category picker → scroll through categories → select one → only those words appear
5. Host → server connect screen → type address → connect → room created → lobby shows player list → settings accessible while waiting
6. Join → server connect → enter room code → wrong code shows error inline → correct code joins
7. Networked: host + joiners, holder selection from participant list, play-again/pick-next-holder flow, peer disconnect handling. Host sees stats + `[P]/[N]/[Q]` hints in one combined box; joiner sees the same stats with a "Waiting for host..." footer until the next `RoleAssignment`.
8. Room stays alive across games — PlayAgain and PickNextHolder work without reconnecting
9. Multi-viewer: 1 host + multiple joiners, host picks a joiner as Holder, all viewers see words
10. Holder rotation: after round, host picks a different Holder via "Pick Next Holder"
11. Joiner disconnect mid-game: viewer leaves, game continues; holder leaves, round ends
12. Room full: 9th joiner gets RoomFull error
9. Ctrl+C during game — terminal restores cleanly
10. `./.history/history.json` (next to the binary) is written after a game
11. `lists/` missing or empty → startup shows a clear on-screen error and exits
12. Dropping a new `.txt` into `lists/` makes it appear in the Word List picker
13. Settings → Import Word List: drop each of a `.csv` (1-col and 2-col with recognized and ambiguous headers), a `.tsv`, a `.json` map, and a newline `.txt` into `imports/` and run the flow end-to-end. Verify: auto-detected columns skip the column picker, ambiguous headers prompt, invalid JSON and unsupported extensions show clean errors, duplicate output names prompt overwrite/auto-suffix/cancel, empty `imports/` shows the absolute path, and the new list appears in the Word List picker after success.
14. Low-time warning: start a round with `game_time ≥ 15s`; verify the timer text and a red blinking border along the terminal edge appear when 10 seconds remain (2Hz blink, ~500ms on/off) and persist through 0. Repeat for solo, host Viewer, host Holder, joiner Viewer, and joiner Holder views. The border and timer color must be red regardless of the active color scheme.
15. Auto-rotate Holder: Settings → enable `Auto-rotate Holder`. Host + 2 joiners → first round, host is Holder (no picker shown). End round → summary shows session total "Session: X / Y" → press Enter → next round, holder rotates to peer1, then peer2, then back to host. Disable the setting mid-session and verify the next round reverts to the normal Play Again / Pick Next Holder / Quit menu. Joiner screens show only the round summary (no session total), and advance on each new `RoleAssignment`.

## Working With Claude

- **Ask before implementing**: Before making changes, ask pertinent clarifying questions to ensure alignment on scope, approach, and intent.
- **Before creating a PR**: Update `README.md` and `CLAUDE.md` to reflect any changes introduced by the branch. Do this before opening the pull request, not after.

### CRITICAL — Branch Protection Rules

**ABSOLUTELY DO NOT push to `main`. NEVER. Under ANY circumstances.**

- **ALL work MUST be done on feature branches.** No exceptions.
- **NEVER commit directly to `main`.** Not even "small" or "trivial" changes.
- **NEVER push to `main`.** Not even if it seems convenient. Not even if you're "just adding a file."
- **ALWAYS create a feature branch FIRST**, then commit, then open a pull request.
- If you find yourself on `main`, **STOP** and switch to a new branch before doing anything.

Violating this rule means lost work, broken workflows, and angry maintainers. **There are ZERO acceptable reasons to push to main.**

**Exception — `TODO.md`:** Changes limited to `TODO.md` may be committed and pushed directly to `main` without a feature branch or pull request. This exception applies *only* when `TODO.md` is the sole modified file in the commit; any commit that also touches other files must follow the standard feature-branch + PR workflow.

## Git Workflow

- Feature branches merged via pull requests.
- Branch naming: descriptive kebab-case names (e.g. `flashing-lights`, `p2p-networked-mode`).
- **All changes go through PRs. Direct pushes to `main` are forbidden** — except for `TODO.md`-only changes, which may be pushed directly to `main`.

## Release Versioning

- `TODO.md` groups items by target release (the `Release` column — e.g. `v0.2`, `v1.0`, `v1.1`).
- **When the final item for a given release is marked ✅, bump the `version` field in every workspace `Cargo.toml` to that release number** — root `Cargo.toml` plus each crate under `crates/` (`client`, `protocol`, `relay`). Keep all workspace crates on the same version.
- The bump belongs in the same feature branch / PR that completes the last TODO item for that release, so the merge to `main` ships the version change alongside the feature.
- After bumping, run `cargo build` to refresh `Cargo.lock` and commit it in the same change.

## Future Plans

- **Game history CLI viewer**: View `./.history/history.json` (next to the binary) from the command line.
- **Configurable key bindings**: Currently hardcoded to y/n/q.

## Word List

`lists/ASOIAF_list.txt` — 356 entries across 5 broad categories (Characters, Weapons & Artifacts, Places, Culture, Lore & Legends). Additional `.txt` files in `lists/` are discovered automatically and appear in the in-game Word List picker. Format:

```
[Category Name]
Entry One
Entry Two
```
