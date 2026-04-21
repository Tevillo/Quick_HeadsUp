# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

An ASOIAF (A Song of Ice and Fire) themed "Guess Up" party game for the terminal, built in Rust. Players see a name/term on screen and press `y` (correct) or `n` (pass) before the timer runs out. Supports solo play and networked multi-player mode (1 host + up to 8 joiners) via a relay server.

## Build & Run

```bash
cargo build --release                          # build all crates
cargo run -p guess_up                          # launches TUI menu

# Relay server
cargo run -p relay                             # binds to 0.0.0.0:7878
```

All game settings (game time, categories, extra-time mode, relay server, etc.) are configured through the interactive TUI menu. Settings persist between sessions in `~/.guess_up_config.json`. No CLI flags needed for the client.

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
| `main.rs` | Word loading with category parsing, game runners (solo/host/join), entry point |
| `config.rs` | `AppConfig` struct, defaults, load/save `~/.guess_up_config.json`, `to_game_config()` |
| `menu.rs` | TUI menu state machine (`menu_loop`), all screens (main, settings, category picker, server connect, join room), game dispatch |
| `types.rs` | `GameEvent`, `UserAction`, `GameMode`, `GameConfig`, `GameResult`, type aliases |
| `game.rs` | `GameState`, game loop (`tokio::select!`), Fisher-Yates shuffle, history saving. `run_game` is the host/solo loop, `run_remote_game` is the joiner's display-only loop |
| `input.rs` | `input_task()` — crossterm `EventStream`, single-keypress in raw mode |
| `timer.rs` | `timer_task()` — 1s interval ticks, bonus-time channel for extra-time mode |
| `render.rs` | `TerminalGuard` (RAII cleanup), `MenuItem` enum, menu rendering, game rendering, flash, countdown, lobby screens, summary output |
| `net.rs` | `NetConnection` (TCP connect/split/reassemble), `NetHandle` (spawn read/write tasks, recoverable shutdown), `OutboundMsg` (Broadcast/SendTo routing) |
| `lobby.rs` | Room creation, host lobby (wait for players with live participant list), holder selection (pick any participant), joiner session loop, post-game menu (play again / pick next holder / quit), connection recovery across games |

The key invariant is that `input.rs` stays separate from `game.rs` to allow swapping input sources.

### Key Implementation Details

- **TerminalGuard**: RAII pattern with `Drop` — ensures raw mode and alternate screen are cleaned up on panic or early exit.
- **Flash race condition**: `flash_screen()` clobbers the display; after 150ms it sends `GameEvent::Redraw` so the game loop re-renders the current word. Both game loops track a `flashing` flag to skip renders while the flash is on screen, preventing the game loop or timer ticks from overwriting the flash effect.
- **Summary rendering**: `TerminalGuard` must be dropped *before* `print_output()` — otherwise the summary prints inside the alternate screen buffer and gets wiped.
- **Connection recovery**: In networked mode, `NetHandle::shutdown()` recovers the TCP reader/writer from background tasks so the connection can be reused across games without reconnecting. Both host and joiner recover connections after each game for multi-round play.
- **Host-authoritative model**: The host owns all game state (words, timer, score). Joiners run `run_remote_game` which only renders based on messages received from the host. Input routing depends on role — Viewer processes `RemoteInput` from the holder's `PeerId`, Holder processes `UserInput`. The host picks who the Holder is from a participant list (can be themselves or any joiner).
- **Multi-viewer rooms**: Rooms support 1 host + up to 8 joiners. The relay server manages a `Vec<Peer>` per room, assigns monotonically increasing PeerIds, and routes messages (broadcast or targeted). Only host disconnect removes the room; joiner disconnect is non-fatal.
- **Joiner post-game**: After a game, the joiner waits for the host's next `RoleAssignment` (signaling a new round) rather than intermediate `PlayAgain`/`PickNextHolder` messages. This avoids a race condition where the net read task could consume messages during shutdown.
- **Menu-driven game dispatch**: `menu_loop` owns the full lifecycle — it runs games internally and loops back to the appropriate screen (server connect, room code) after each game ends, preserving menu state.
- **Settings persistence**: `AppConfig` is loaded from `~/.guess_up_config.json` on startup and saved after each menu exit or game. `#[serde(default)]` ensures forward compatibility.
- **Address validation**: Relay server addresses are validated (host:port format, numeric port 1-65535) before connection attempts. Errors display inline in red on the input screen.
- **Word loading**: Parses `[Category]` headers, trims lines, deduplicates via `HashSet` (case-insensitive).
- **History**: Saved to `~/.guess_up_history.json` via serde.

### Protocol

The `protocol` crate defines length-prefixed JSON framing over TCP (`read_frame`/`write_frame`, max 64KB). Peers are identified by `PeerId` (u8); host is always `HOST_PEER_ID` (0), joiners get 1, 2, 3, etc. assigned by the relay. Three message layers:
- **ClientMessage**: client → relay (CreateRoom, JoinRoom, GameData { msg, target }, Disconnect, Pong). `target: None` broadcasts, `target: Some(id)` sends to a specific peer.
- **RelayMessage**: relay → client (RoomCreated, PeerJoined { peer_id }, JoinedRoom { peer_id }, PeerList { peers }, GameData { msg, from }, PeerDisconnected { peer_id }, Ping)
- **GameMessage**: peer ↔ peer (forwarded through relay) — RoleAssignment { holder_id }, word updates, timer sync, score, input, post-game actions (PlayAgain, PickNextHolder, QuitSession)

## Testing

Manual play-testing is the primary test strategy. Key scenarios to verify:

1. `cargo run -p guess_up` — main menu renders, arrow/hjkl navigation works
2. Settings → change game_time → exit → re-run → value persisted in `~/.guess_up_config.json`
3. Solo → plays full game → returns to main menu
4. Category picker → scroll through categories → select one → only those words appear
5. Host → server connect screen → type address → connect → room created → lobby shows player list → settings accessible while waiting
6. Join → server connect → enter room code → wrong code shows error inline → correct code joins
7. Networked: host + joiners, holder selection from participant list, play-again/pick-next-holder flow, peer disconnect handling
8. Room stays alive across games — PlayAgain and PickNextHolder work without reconnecting
9. Multi-viewer: 1 host + multiple joiners, host picks a joiner as Holder, all viewers see words
10. Holder rotation: after round, host picks a different Holder via "Pick Next Holder"
11. Joiner disconnect mid-game: viewer leaves, game continues; holder leaves, round ends
12. Room full: 9th joiner gets RoomFull error
9. Ctrl+C during game — terminal restores cleanly
10. `~/.guess_up_history.json` is written after a game

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

## Future Plans

- **Game history CLI viewer**: View `~/.guess_up_history.json` from the command line.
- **Configurable key bindings**: Currently hardcoded to y/n/q.

## Word List

`files/ASOIAF_list.txt` — 420 entries across 25 categories (House Stark, Dragons, Valyrian Steel, Theories, etc.). Format:

```
[Category Name]
Entry One
Entry Two
```
