# Multi-Viewer Support Plan

## Context

Currently, networked games support exactly 2 peers (1 host + 1 joiner). The relay server has a single `joiner_tx: Option<Sender>` per room, and the client assumes a 1:1 relationship everywhere. This plan extends networked mode to support **1 host + up to 8 joiners** in a single room, enabling the party game experience where multiple people see the word and give clues while one person guesses.

**Key design decisions:**
- Extend existing networked mode (not a separate mode) -- any room supports 1-8 joiners
- Host picks who the Holder is from a participant list (can be themselves or any joiner)
- Between rounds, host picks the next Holder (enables rotation)
- Spectator mode (separate TODO item) is removed from the TODO list

---

## Phase 1: Protocol Changes (`crates/protocol/src/lib.rs`)

Add a PeerId type and update all message enums. Host is always PeerId 0, joiners get 1, 2, 3, etc. assigned by the relay.

**Add:**
```rust
pub type PeerId = u8;
pub const HOST_PEER_ID: PeerId = 0;
```

**`ClientMessage`** -- change `GameData` from tuple to struct variant with optional target:
```rust
pub enum ClientMessage {
    CreateRoom,
    JoinRoom { code: String },
    GameData { msg: GameMessage, target: Option<PeerId> }, // None = broadcast (host) or send to host (joiner)
    Disconnect,
    Pong,
}
```

**`RelayMessage`** -- add peer IDs and a peer list message:
```rust
pub enum RelayMessage {
    RoomCreated { code: String },
    PeerJoined { peer_id: PeerId },           // was unit variant
    JoinedRoom { peer_id: PeerId },           // was unit variant; tells joiner their ID
    PeerList { peers: Vec<PeerId> },          // NEW: sent to new joiner (who's already here)
    GameData { msg: GameMessage, from: PeerId }, // was tuple; now carries sender ID
    PeerDisconnected { peer_id: PeerId },     // was unit variant
    Error(RelayError),
    Ping,
}
```

**`GameMessage`** -- update role assignment and post-game actions:
```rust
// Change:
RoleAssignment { host_role: Role }  -->  RoleAssignment { holder_id: PeerId }
// Each recipient computes their own role: holder_id == my_id ? Holder : Viewer

// Remove:
SwapRoles  // no longer meaningful with N players

// Add:
PickNextHolder  // host signals a new holder selection round
```

---

## Phase 2: Relay Server (`crates/relay/src/main.rs`)

### 2a. Room struct

```rust
struct Peer {
    tx: mpsc::Sender<RelayMessage>,
    peer_id: PeerId,
}

struct Room {
    host_tx: mpsc::Sender<RelayMessage>,
    peers: Vec<Peer>,         // was: joiner_tx: Option<Sender>
    next_peer_id: PeerId,     // starts at 1, increments monotonically (never reuses IDs)
    created_at: Instant,
}
```

Add helper methods on `Room`:
- `broadcast_to_peers(&self, msg, exclude: Option<PeerId>)` -- send to all peers, optionally excluding one
- `send_to_peer(&self, peer_id, msg) -> bool` -- send to a specific peer
- `remove_peer(&mut self, peer_id)` -- remove a peer from the vec

### 2b. `create_room()` 
Initialize `peers: Vec::new()`, `next_peer_id: 1`. No other changes.

### 2c. `join_room()`
- Check `peers.len() < 8` (was `joiner_tx.is_some()`)
- Assign `peer_id = next_peer_id; next_peer_id += 1`
- Push `Peer { tx, peer_id }` onto `peers`
- Send `PeerJoined { peer_id }` to the host AND all existing peers
- Return `(Arc<Mutex<Room>>, peer_id)` so the handler can send `JoinedRoom` and `PeerList`

### 2d. `handle_host()` -- major rewrite
The current code blocks waiting for exactly one `PeerJoined` before entering `run_forwarding`. Replace with an immediate main loop:

1. Create room, send `RoomCreated` to host
2. Enter select loop immediately (no waiting for first joiner):
   - `rx.recv()` -> write to host TCP (PeerJoined notifications come through here)
   - `read_frame` from host TCP:
     - `GameData { msg, target: None }` -> lock room, broadcast to all peers
     - `GameData { msg, target: Some(id) }` -> lock room, send to that specific peer
     - `Disconnect` -> notify all peers with `PeerDisconnected { peer_id: 0 }`, remove room, break
   - `ping_interval` -> send Ping

The host handler needs `Arc<Mutex<Room>>` to access the peer list for broadcasting.

### 2e. `handle_joiner()` -- moderate rewrite
1. `join_room()` returns `(room, peer_id)`
2. Send `JoinedRoom { peer_id }` and `PeerList { peers }` to the joiner
3. Enter select loop:
   - `rx.recv()` -> write to joiner TCP
   - `read_frame` from joiner TCP:
     - `GameData { msg, .. }` -> send to `host_tx` as `RelayMessage::GameData { msg, from: peer_id }`
     - `Disconnect` -> remove peer from room, notify host + remaining peers, break
   - `ping_interval` -> send Ping
4. On joiner disconnect: remove peer, notify others. Room stays alive.

### 2f. `run_forwarding()` -- removed
Replaced by the separate host/joiner loops above. The asymmetric routing (host broadcasts, joiner sends to host) makes a shared function awkward.

### 2g. Room lifecycle changes
- Only host disconnect removes the room (currently either peer removes it)
- Joiner disconnect: remove from `peers`, notify host + remaining peers
- `reap_stale_rooms()`: no changes needed

---

## Phase 3: Client Net Layer (`crates/client/src/net.rs`, `crates/client/src/types.rs`)

### 3a. Outbound message enum (`net.rs`)
```rust
pub enum OutboundMsg {
    Broadcast(GameMessage),
    SendTo(PeerId, GameMessage),
}
```

Change `NetHandle::outbound_tx` from `mpsc::Sender<GameMessage>` to `mpsc::Sender<OutboundMsg>`.

### 3b. `net_write_task()` 
Map `OutboundMsg::Broadcast(msg)` to `ClientMessage::GameData { msg, target: None }` and `OutboundMsg::SendTo(id, msg)` to `ClientMessage::GameData { msg, target: Some(id) }`.

### 3c. `net_read_task()` / `translate_game_message()`
Handle new `RelayMessage` variants:
- `GameData { msg, from }` -> translate `msg` as before; for `PlayerInput`, produce `GameEvent::RemoteInput(from, action)` with the `from` peer ID (see types changes below)
- `PeerJoined { peer_id }` -> `GameEvent::PeerJoined(peer_id)`
- `PeerDisconnected { peer_id }` -> `GameEvent::PeerDisconnected(peer_id)`
- `PeerList { peers }` -> `GameEvent::PeerList(peers)`

### 3d. GameEvent changes (`types.rs`)
```rust
pub enum GameEvent {
    // Existing
    UserInput(UserAction),
    TimerTick(u64),
    TimerExpired,
    Redraw,

    // Network -- updated with PeerId
    RemoteInput(PeerId, UserAction),    // was RemoteInput(UserAction)
    NetWordUpdate(String),
    NetTimerSync(u64),
    NetScoreUpdate { score: usize, total: usize },
    NetFlash(FlashKind),
    NetTimerExpired,
    NetGameOver(NetGameResult),
    PeerDisconnected(PeerId),           // was PeerDisconnected (unit)

    // New
    PeerJoined(PeerId),
    PeerList(Vec<PeerId>),
}
```

---

## Phase 4: Client Lobby (`crates/client/src/lobby.rs`)

### 4a. `wait_for_peer()` -> `wait_for_players()`
Replace the current function that returns `bool` on first PeerJoined. New behavior:
- Renders a live lobby showing room code, connected player count, and player list
- Accumulates `Vec<PeerId>` as `PeerJoined` events arrive
- Removes peers on `PeerDisconnected`
- Shows "Start Game" action (enabled when >= 1 joiner present) and "Settings" / "Disconnect"
- Returns `Some(Vec<PeerId>)` when host presses Start, or `None` on disconnect

```
          HOST LOBBY

      Room: ABCDE

      Players: 3/9
        Host (you)
        Player 1
        Player 2

      > Start Game
        Settings
        Disconnect
```

### 4b. `select_role()` -> `select_holder()`
Replace the Viewer/Holder binary choice with a participant picker:
- Lists all participants: "Host (you)" + "Player N" for each joiner PeerId
- Host navigates and selects who will be the Holder
- Returns `PeerId` (0 for host, or the joiner's ID)

```
      CHOOSE THE HOLDER

      > Host (you)
        Player 1
        Player 2

        Settings
```

### 4c. `run_host_session()` changes
Update the main game loop:
1. `wait_for_players()` -> get participant list
2. `select_holder()` -> get holder PeerId
3. Broadcast `RoleAssignment { holder_id }` to all joiners (via `Broadcast`)
4. Wait for `RoleAccepted` from all joiners (track `HashSet<PeerId>`)
5. Broadcast `GameStart(net_config)`
6. Run game via `run_host_game()` (pass `holder_peer_id`)
7. Post-game: replace `PostGameAction::SwapRoles` with `PostGameAction::PickNextHolder`

The `RoleAssignment` is broadcast to all. Each joiner computes: `if holder_id == my_id { Holder } else { Viewer }`.

### 4d. `run_host_game()` changes
Pass `holder_peer_id: Option<PeerId>` through to `run_game()`. If host is the Holder, `holder_peer_id` is `None` (process local input). If a joiner is the Holder, `holder_peer_id` is `Some(id)`.

### 4e. Post-game menu
Replace `SwapRoles` option:
```rust
enum PostGameAction {
    PlayAgain,       // same holder
    PickNextHolder,  // return to holder selection screen
    Quit,
}
```
- "Play Again (same holder)" -> send `PlayAgain` broadcast, loop back to game
- "Pick Next Holder" -> send `PickNextHolder` broadcast, loop back to `select_holder()`
- "Quit" -> send `QuitSession` broadcast

### 4f. `run_joiner_session()` changes
- Store `my_id: PeerId` from `JoinedRoom { peer_id }` response
- On `RoleAssignment { holder_id }`: compute role as `if holder_id == my_id { Holder } else { Viewer }`
- On `PeerDisconnected(0)` (host): show "Host disconnected", exit
- On `PeerDisconnected(other)`: ignore (host handles participant management)
- Between rounds: wait for next `RoleAssignment` (same as today)

### 4g. `try_join_room()` changes
Parse `JoinedRoom { peer_id }` instead of unit `JoinedRoom`. Return `(NetConnection, PeerId)`.

---

## Phase 5: Game Loop (`crates/client/src/game.rs`)

### 5a. `run_game()` signature
Add `holder_peer_id: Option<PeerId>` parameter. This identifies which remote peer is the Holder (None if host is Holder or solo mode).

### 5b. Input routing
Update the match in `run_game()`:
```rust
let should_process = match (local_role, &event) {
    (None, GameEvent::UserInput(_)) => true,                           // Solo
    (Some(Role::Viewer), GameEvent::RemoteInput(pid, _))
        if Some(*pid) == holder_peer_id => true,                       // Remote holder's input
    (Some(Role::Holder), GameEvent::UserInput(_)) => true,             // Local holder's input
    (Some(Role::Viewer), GameEvent::UserInput(UserAction::Quit)) => true, // Local viewer quit
    _ => false,
};
```

### 5c. `net_tx` calls
All `net_tx.send(GameMessage::...)` become `net_tx.send(OutboundMsg::Broadcast(GameMessage::...))` since game state updates go to all peers.

### 5d. `PeerDisconnected` handling
- `PeerDisconnected(pid)` where `pid == holder_peer_id`: end the round (holder left)
- `PeerDisconnected(pid)` where `pid` is a viewer: continue playing (non-fatal)

### 5e. `run_remote_game()` -- minimal changes
- `RemoteInput` variant now carries PeerId but the joiner doesn't receive RemoteInput events (those come from the host's perspective). No change needed.
- `PeerDisconnected` now carries PeerId; only react to PeerId 0 (host).
- All other behavior is unchanged.

---

## Phase 6: Rendering (`crates/client/src/render.rs`)

Add new render functions:
- `render_host_lobby()` -- multi-player waiting room with participant list and Start/Settings/Disconnect
- `render_holder_picker()` -- participant list as selectable items for holder selection
- `render_post_game_multi()` -- post-game menu with "Play Again" / "Pick Next Holder" / "Quit"

Existing render functions (`render_question`, `render_holder_view`, `render_role_assigned`, `render_joined_room`, `render_post_game_menu`, flash functions) need no changes -- each client renders locally based on their assigned role.

---

## Phase 7: Cleanup

- Remove `SwapRoles` from `GameMessage` and all match arms
- Remove "Spectator mode" line from `TODO.md`
- Update `CLAUDE.md` architecture docs and `README.md`

---

## Implementation Order

1. **Protocol crate** -- all type changes first (compile errors will guide client/relay updates)
2. **Relay server** -- Room struct, join/create/handle refactors
3. **Client types.rs** -- GameEvent updates
4. **Client net.rs** -- OutboundMsg, read/write task updates
5. **Client lobby.rs** -- wait_for_players, select_holder, session loops, post-game
6. **Client game.rs** -- input routing, outbound changes, disconnect handling
7. **Client render.rs** -- new lobby/picker/post-game screens
8. **Cleanup** -- remove SwapRoles, update TODO/CLAUDE.md/README.md

## Files Modified

| File | Change Scope |
|------|-------------|
| `crates/protocol/src/lib.rs` | PeerId type, all message enum updates |
| `crates/relay/src/main.rs` | Room struct, join/create, handle_host/joiner rewrite, remove run_forwarding |
| `crates/client/src/types.rs` | GameEvent enum updates |
| `crates/client/src/net.rs` | OutboundMsg enum, read/write task updates |
| `crates/client/src/lobby.rs` | wait_for_players, select_holder, session loops, post-game menu |
| `crates/client/src/game.rs` | run_game signature + input routing, run_remote_game PeerDisconnected |
| `crates/client/src/render.rs` | New render functions for lobby/picker/post-game |
| `TODO.md` | Remove spectator mode line |
| `CLAUDE.md` | Update architecture docs |
| `README.md` | Update feature description |

## Files NOT Modified

| File | Reason |
|------|--------|
| `crates/client/src/input.rs` | Produces UserInput events unchanged |
| `crates/client/src/timer.rs` | Timer is host-local, unaffected |
| `crates/client/src/config.rs` | Game settings still host-only |
| `crates/client/src/menu.rs` | Menu structure unchanged (Solo/Host/Join/Settings/Quit); lobby changes are internal to lobby.rs |
| `crates/client/src/main.rs` | run_host/run_join just call into lobby |

## Verification

1. `cargo build --release` -- all crates compile clean
2. `cargo clippy` -- no warnings
3. `cargo fmt --check` -- formatting clean
4. **Solo mode regression**: solo play works exactly as before
5. **2-player regression**: 1 host + 1 joiner works (special case of multi-viewer)
6. **Multi-viewer**: 1 host + 3 joiners, host picks a joiner as Holder, play a round
7. **Holder rotation**: after round, host picks a different Holder
8. **Joiner disconnect**: viewer disconnects mid-game, game continues for remaining players
9. **Holder disconnect**: holder joiner disconnects mid-game, round ends
10. **Host disconnect**: all joiners see "Host disconnected" and exit
11. **Room full**: 9th joiner gets `RoomFull` error
12. **Late joiner**: joiner connects while host is in lobby, appears in participant list
