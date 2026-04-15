# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

An ASOIAF (A Song of Ice and Fire) themed "Guess Up" party game for the terminal, built in Rust. Players see a name/term on screen and press `y` (correct) or `n` (pass) before the timer runs out. Supports solo play and networked two-player mode via a relay server.

## Build & Run

```bash
cargo build --release                          # build all crates
cargo run -p guess_up                          # solo mode, default 60s game
cargo run -p guess_up -- -x --bonus-seconds 3  # extra-time mode
cargo run -p guess_up -- --category "House Stark"  # filter by category

# Networked mode
cargo run -p guess_up -- host --relay server:7878
cargo run -p guess_up -- join --relay server:7878 --code ABCDE

# Relay server
cargo run -p relay                             # binds to 0.0.0.0:7878
```

CLI flags: `-g` (game time), `-s` (skip countdown), `-l` (last unlimited), `-x` (extra time), `--bonus-seconds`, `-w` (word file), `--category`. Flags go before the subcommand (`host`/`join`).

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
  relay/      # standalone relay server binary — room management, message forwarding, heartbeat pings
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
| `main.rs` | CLI args (clap), word loading with category parsing, channel setup, task spawning |
| `types.rs` | `GameEvent`, `UserAction`, `GameMode`, `GameConfig`, `GameResult`, type aliases |
| `game.rs` | `GameState`, game loop (`tokio::select!`), Fisher-Yates shuffle, history saving. `run_game` is the host/solo loop, `run_remote_game` is the joiner's display-only loop |
| `input.rs` | `input_task()` — crossterm `EventStream`, single-keypress in raw mode |
| `timer.rs` | `timer_task()` — 1s interval ticks, bonus-time channel for extra-time mode |
| `render.rs` | `TerminalGuard` (RAII cleanup), rendering, flash, countdown, lobby screens, summary output |
| `net.rs` | `NetConnection` (TCP connect/split/reassemble), `NetHandle` (spawn read/write tasks, recoverable shutdown) |
| `lobby.rs` | Room creation, role selection, joiner handshake, post-game menu (play again / swap roles / quit) |

The key invariant is that `input.rs` stays separate from `game.rs` to allow swapping input sources.

### Key Implementation Details

- **TerminalGuard**: RAII pattern with `Drop` — ensures raw mode and alternate screen are cleaned up on panic or early exit.
- **Flash race condition**: `flash_screen()` clobbers the display; after 300ms it sends `GameEvent::Redraw` so the game loop re-renders the current word. Both game loops track a `flashing` flag to skip renders while the flash is on screen, preventing the game loop or timer ticks from overwriting the flash effect.
- **Summary rendering**: `TerminalGuard` must be dropped *before* `print_output()` — otherwise the summary prints inside the alternate screen buffer and gets wiped.
- **Connection recovery**: In networked mode, `NetHandle::shutdown()` recovers the TCP reader/writer from background tasks so the connection can be reused across games without reconnecting.
- **Host-authoritative model**: The host owns all game state (words, timer, score). The joiner runs `run_remote_game` which only renders based on messages received from the host. Input routing depends on role — Viewer processes `RemoteInput`, Holder processes `UserInput`.
- **Word loading**: Parses `[Category]` headers, trims lines, deduplicates via `HashSet` (case-insensitive).
- **History**: Saved to `~/.guess_up_history.json` via serde.

### Protocol

The `protocol` crate defines length-prefixed JSON framing over TCP (`read_frame`/`write_frame`, max 64KB). Three message layers:
- **ClientMessage**: client → relay (CreateRoom, JoinRoom, GameData, Disconnect, Pong)
- **RelayMessage**: relay → client (RoomCreated, PeerJoined, GameData, PeerDisconnected, Ping)
- **GameMessage**: peer ↔ peer (forwarded through relay) — role assignment, word updates, timer sync, score, input, post-game actions

## Testing

Manual play-testing is the primary test strategy. Key scenarios to verify:

1. `cargo run -p guess_up` — normal mode works end-to-end
2. `cargo run -p guess_up -- --last-unlimited` — last question gets infinite time
3. `cargo run -p guess_up -- --extra-time --bonus-seconds 3` — bonus time adds correctly
4. `cargo run -p guess_up -- --category "House Stark"` — category filtering works
5. Ctrl+C during game — terminal restores cleanly
6. `~/.guess_up_history.json` is written after a game
7. Networked mode: host + join, role selection, play-again flow, peer disconnect handling

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

## Git Workflow

- Feature branches merged via pull requests.
- Branch naming: descriptive kebab-case names (e.g. `flashing-lights`, `p2p-networked-mode`).
- **All changes go through PRs. Direct pushes to `main` are forbidden.**

## Future Plans

- **Game history CLI viewer**: View `~/.guess_up_history.json` from the command line.
- **Multi-round / replay**: "Play again?" prompt after a solo round.
- **Configurable key bindings**: Currently hardcoded to y/n/q.

## Word List

`files/ASOIAF_list.txt` — 420 entries across 25 categories (House Stark, Dragons, Valyrian Steel, Theories, etc.). Format:

```
[Category Name]
Entry One
Entry Two
```
