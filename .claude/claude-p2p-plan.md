# Networked P2P Mode via Server Relay

## Context

The game is currently single-device only. The goal is to add a networked two-player mode where one person (Viewer) sees the word and gives verbal clues, while the other person (Holder) guesses and presses y/n — just like the real Guess Up game but across two terminals on different networks. A relay server running on the user's always-online server bridges the two clients. Players can switch roles between games within a session.

## Architecture Overview

```
┌──────────────┐         ┌──────────────┐         ┌──────────────┐
│   Client A   │◄──TCP──►│ Relay Server │◄──TCP──►│   Client B   │
│  (Host)      │         │ (user's VPS) │         │  (Joiner)    │
│              │         │              │         │              │
│ Game logic   │         │ Room mgmt    │         │ Render loop  │
│ Timer        │         │ Byte forward │         │ Input fwd    │
│ Word list    │         │              │         │              │
└──────────────┘         └──────────────┘         └──────────────┘
```

- **Host** owns all game state (words, timer, score). Runs the full game loop.
- **Joiner** runs a render-only loop. Receives display updates, forwards input.
- **Relay** manages rooms and forwards opaque game messages. Knows nothing about game logic.

## Project Structure — Cargo Workspace

Convert the single crate into a workspace with three crates:

```
Guess-Up/
  Cargo.toml                  # workspace root
  files/ASOIAF_list.txt
  crates/
    protocol/                 # shared message types + TCP framing
      Cargo.toml              # deps: serde, serde_json, tokio (io-util, net)
      src/lib.rs
    relay/                    # standalone relay server binary
      Cargo.toml              # deps: protocol, tokio (full), rand, tracing, clap
      src/main.rs
    client/                   # the game (existing code + networking)
      Cargo.toml              # deps: protocol + all existing deps
      src/
        main.rs               # updated with subcommands
        types.rs              # extended with network events
        game.rs               # host game loop gains net outbound
        input.rs              # unchanged
        timer.rs              # unchanged
        render.rs             # gains holder_view + lobby renders
        net.rs                # NEW: network task
        lobby.rs              # NEW: room setup + role selection UI
```

Existing `src/` moves to `crates/client/src/`. Solo mode stays fully functional — no regressions.

---

## Step 1: Workspace Restructure

Move the existing single-package project into a workspace layout.

- Root `Cargo.toml` becomes a `[workspace]` manifest with `members = ["crates/protocol", "crates/relay", "crates/client"]`
- Existing `src/` moves to `crates/client/src/`
- Existing dependencies move to `crates/client/Cargo.toml`
- `files/` stays at the repo root; the client references it with a relative path (update default in clap arg)
- Verify: `cargo build -p guess_up` and `cargo run -p guess_up` work identically to today

**Files modified:** `Cargo.toml` (rewrite), new `crates/client/Cargo.toml`, `crates/client/src/main.rs` (word file default path)

---

## Step 2: Protocol Crate (`crates/protocol/`)

Shared types and TCP framing used by both relay and client.

### Message Framing

Length-prefixed binary over TCP: `[4 bytes u32 BE length][JSON payload]`. JSON because traffic is tiny (few msgs/sec, <1KB each), serde_json is already a dep, and it aids debugging.

Framing utility functions:
- `write_frame<T: Serialize>(writer, msg)` — serialize to JSON, write length + payload
- `read_frame<T: DeserializeOwned>(reader)` — read length, read payload, deserialize. Reject frames >64KB.

### Message Types

```rust
// Client → Relay
enum ClientMessage {
    CreateRoom,
    JoinRoom { code: String },
    GameData(GameMessage),    // opaque to relay
    Disconnect,
    Pong,
}

// Relay → Client
enum RelayMessage {
    RoomCreated { code: String },
    PeerJoined,
    JoinedRoom,
    GameData(GameMessage),    // forwarded from other peer
    PeerDisconnected,
    Error(RelayError),        // RoomNotFound, RoomFull, InvalidCode, ServerFull
    Ping,
}

// Peer ↔ Peer (forwarded through relay)
enum GameMessage {
    // Lobby
    RoleAssignment { host_role: Role },
    RoleAccepted,
    GameStart(NetGameConfig),

    // In-game: host → remote
    WordUpdate { word: String },
    TimerSync { seconds_left: u64 },
    ScoreUpdate { score: usize, total: usize },
    Flash(FlashKind),
    TimerExpired,
    GameOver(NetGameResult),

    // In-game: remote → host
    PlayerInput(NetUserAction),

    // Post-game
    PlayAgain,
    SwapRoles,
    QuitSession,
}

enum Role { Viewer, Holder }
enum FlashKind { Correct, Incorrect }
enum NetUserAction { Correct, Pass, Quit }
```

**Files:** `crates/protocol/Cargo.toml`, `crates/protocol/src/lib.rs`

---

## Step 3: Relay Server (`crates/relay/`)

A small, stateless (re: game logic) TCP server. ~200-300 lines.

### Core Logic

1. Listen on configurable `bind` address (default `0.0.0.0:3000`)
2. On new TCP connection, read first `ClientMessage`:
   - `CreateRoom` → generate 5-char uppercase room code, store room with host's write channel, respond `RoomCreated { code }`
   - `JoinRoom { code }` → look up room, respond `JoinedRoom` to joiner + `PeerJoined` to host
3. Once both peers connected: spawn two forwarding tasks that pipe `GameData` between them
4. On disconnect/EOF: send `PeerDisconnected` to the other peer, clean up room
5. Background reaper task: every 60s, remove rooms older than 1 hour
6. Heartbeat: send `Ping` every 30s, expect `Pong` within 10s

### Internal State

```rust
struct RelayServer {
    rooms: RwLock<HashMap<String, Arc<Mutex<Room>>>>,
}
struct Room {
    host_tx: mpsc::Sender<RelayMessage>,
    joiner_tx: Option<mpsc::Sender<RelayMessage>>,
    created_at: Instant,
}
```

### CLI Args (clap)

`--bind`, `--max-rooms`, `--room-timeout`

**Files:** `crates/relay/Cargo.toml`, `crates/relay/src/main.rs`

---

## Step 4: Client CLI Changes (`crates/client/src/main.rs`)

Add clap subcommands. Default (no subcommand) runs solo mode for backwards compat.

```
guess_up                                    # solo mode (existing behavior)
guess_up host --relay addr:port             # create room, display code, wait for peer
guess_up join --relay addr:port --code XXXX # join existing room
```

Game flags (`-g`, `-s`, `-l`, `-x`, `--bonus-seconds`, `-w`, `--category`) apply to solo and host modes. The joiner receives config from the host.

**Files modified:** `crates/client/src/main.rs`

---

## Step 5: Network Task (`crates/client/src/net.rs`)

New module handling the TCP connection to the relay and message translation.

### `net_task` function

- Connects to relay server
- Handles room creation/joining (sends `CreateRoom`/`JoinRoom`, waits for response)
- Splits into two concurrent loops:
  - **Read loop**: reads `RelayMessage` from relay → translates `GameData(GameMessage)` into `GameEvent` variants → sends to the shared `event_tx` channel
  - **Write loop**: reads from `net_outbound_rx` channel → wraps as `ClientMessage::GameData` → writes to relay
- Handles `Ping`/`Pong` heartbeat
- On EOF or error: sends `GameEvent::PeerDisconnected` to event channel

### New `GameEvent` variants in `types.rs`

```rust
enum GameEvent {
    // Existing
    UserInput(UserAction),
    TimerTick(u64),
    TimerExpired,
    Redraw,

    // New: network events
    RemoteInput(UserAction),
    NetWordUpdate(String),
    NetTimerSync(u64),
    NetScoreUpdate { score: usize, total: usize },
    NetFlash(FlashKind),
    NetTimerExpired,
    NetGameOver(NetGameResult),
    PeerDisconnected,
}
```

**Files:** new `crates/client/src/net.rs`, modified `crates/client/src/types.rs`

---

## Step 6: Lobby & Role Selection (`crates/client/src/lobby.rs`)

New module for the pre-game and post-game screens, rendered in the alternate screen buffer.

### Pre-game (host)
- Display room code in a box: `"Room: STARK — Waiting for opponent..."`
- On `PeerJoined`: show role selection menu (V for Viewer, H for Holder)
- Send `RoleAssignment` to peer, wait for `RoleAccepted`, then send `GameStart`

### Pre-game (joiner)
- Display `"Joined room STARK — Waiting for host..."`
- On `RoleAssignment`: display assigned role, send `RoleAccepted`, wait for `GameStart`

### Post-game (both)
- Display summary, then menu: `[P] Play again  [S] Swap roles  [Q] Quit`
- Exchange `PlayAgain`/`SwapRoles`/`QuitSession` messages
- On agreement: loop back to game (host reshuffles words) or exit

**Files:** new `crates/client/src/lobby.rs`

---

## Step 7: Host Game Loop Networking (`crates/client/src/game.rs`)

Modify `run_game` to accept an optional network outbound channel and a `Role`:

```rust
pub async fn run_game(
    config: GameConfig,
    words: Vec<String>,
    rx: EventReceiver,
    bonus_tx: Option<BonusSender>,
    flash_tx: EventSender,
    net_tx: Option<mpsc::Sender<GameMessage>>,  // NEW
    local_role: Option<Role>,                    // NEW: None = solo
) -> GameSummary
```

Changes to the event match:
- **Input routing by role**: If `local_role == Some(Viewer)`, process `RemoteInput` (holder is remote). If `local_role == Some(Holder)`, process `UserInput` (holder is local). If `None`, process `UserInput` (solo mode).
- **After each state change**, if `net_tx` is `Some`, send the corresponding `GameMessage`:
  - Advance word → `WordUpdate` (only if remote is Viewer)
  - Timer tick → `TimerSync`
  - Score change → `ScoreUpdate`
  - Correct/Pass → `Flash`
  - Timer expired → `TimerExpired`
  - Game end → `GameOver`
- Handle `PeerDisconnected` → end game gracefully

**Files modified:** `crates/client/src/game.rs`

---

## Step 8: Remote Render Loop (`crates/client/src/game.rs`)

New function for the non-host client:

```rust
pub async fn run_remote_game(
    role: Role,
    rx: EventReceiver,
    net_tx: mpsc::Sender<GameMessage>,
    flash_tx: EventSender,
) -> GameSummary
```

This does NOT run the game timer or own the word list. It:
- Receives `NetWordUpdate` → renders the word (Viewer only)
- Receives `NetTimerSync` → updates timer display
- Receives `NetScoreUpdate` → updates score display
- Receives `NetFlash` → triggers flash effect
- On local `UserInput` (if Holder) → sends `PlayerInput` over network
- Receives `NetGameOver` → displays summary, returns

**Files modified:** `crates/client/src/game.rs`

---

## Step 9: Role-Aware Rendering (`crates/client/src/render.rs`)

Add new rendering functions:

### `render_holder_view(seconds_left, score, term_size)`
Shows timer and score in a box, with `"Press [Y] Correct  [N] Pass"` prompt. Does NOT show the word — shows `"???"` or `"GUESS!"` as placeholder.

### Lobby renders
- `render_waiting_for_peer(room_code, term_size)` — host waiting screen
- `render_joined_room(room_code, term_size)` — joiner waiting screen  
- `render_role_select(term_size)` — host picks Viewer/Holder
- `render_role_assigned(role, term_size)` — joiner sees their role
- `render_post_game_menu(term_size)` — play again / swap / quit

Existing `render_question`, `render_question_unlimited`, `flash_*`, `print_output` are unchanged.

**Files modified:** `crates/client/src/render.rs`

---

## Step 10: Polish & Error Handling

- Relay unreachable → friendly error message, exit
- Invalid room code → `"Room not found"`, exit
- Disconnect mid-game → `"Opponent disconnected"`, save partial result, show summary
- Relay server crash → same as disconnect (TCP error → `PeerDisconnected`)
- Ctrl+C → `TerminalGuard` RAII cleanup still works (unchanged)

---

## Implementation Order

| Step | What | Testable? |
|------|-------|-----------|
| 1 | Workspace restructure | `cargo run -p guess_up` works identically |
| 2 | Protocol crate | Unit tests for serialization round-trips |
| 3 | Relay server | Manual test: two `nc` / test clients create + join rooms |
| 4 | CLI subcommands | `guess_up host/join` parse args, connect to relay, show room code |
| 5 | net.rs + types.rs | Two clients connect, see "peer joined" |
| 6 | lobby.rs | Role selection works, both sides agree on roles |
| 7 | Host game loop networking | Host plays full game, remote receives messages (not rendered yet) |
| 8 | Remote render loop | Full networked game: both players see their role-appropriate view |
| 9 | Render additions | Holder view, lobby screens |
| 10 | Error handling + post-game | Disconnect handling, play again / swap roles / quit |

## Verification

1. **Solo mode regression**: `cargo run -p guess_up` — all existing scenarios still work
2. **Relay**: Start relay, verify room create/join with two terminals
3. **Full networked game**: Host creates room, joiner enters code, roles assigned, play a full game, verify both sides show correct views
4. **Role swap**: After game, swap roles, play again — verify views switch
5. **Disconnect**: Kill one client mid-game, verify the other handles it gracefully
6. **Ctrl+C**: Verify terminal restores cleanly on both sides
7. `cargo fmt` and `cargo clippy` clean
