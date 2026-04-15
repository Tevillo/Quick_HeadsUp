# Guess Up! - ASOIAF Edition

A terminal-based "Guess Up" party game themed around A Song of Ice and Fire. Play solo (hold the screen to your forehead while friends give clues) or networked (two players on different machines connected through a relay server).

Press `y` for correct, `n` to pass — no Enter needed.

## Quickstart

Requires [Rust](https://www.rust-lang.org/tools/install). If you run into version issues, run `rustup toolchain install stable`.

```bash
# Build everything (client + relay server)
cargo build --release

# Launch the game
cargo run -p guess_up
```

An interactive TUI menu lets you configure everything — game time, categories, extra-time mode, relay server address — without any command-line flags. Settings persist between sessions in `~/.guess_up_config.json`.

Press `q` at any time to quit. The terminal always restores cleanly, even on Ctrl+C.

## Solo Mode

Select **Solo Game** from the main menu. Adjust settings (game time, category, extra-time mode, etc.) via the **Settings** screen before starting.

## Networked Mode

Two players connect through a relay server. One player **hosts** a room (owns the game state, timer, and word list) and the other **joins** with a room code.

### Hosting a Game

Select **Host Game** from the main menu, then enter your relay server address (e.g. `your-server:7878`). The host lobby shows the room code and lets you adjust **Settings** while waiting for an opponent. Once a joiner connects, pick your role:

- **Viewer** — sees the word on screen and gives verbal clues
- **Holder** — guesses based on clues and presses `y`/`n`

After each game, the host gets a post-game menu to play again, swap roles, or quit. The room stays alive across games — no need to reconnect.

### Joining a Game

Select **Join Game** from the main menu, enter the relay server address, then type the room code the host gave you. If the code is wrong, the error appears inline so you can fix it and retry. After the game, you stay in the lobby and can join another round when the host starts one.

### Relay Server Setup

The relay is a lightweight TCP server that forwards messages between the two players. It knows nothing about game logic — all state lives on the host client.

**Build and deploy:**

```bash
# Build
cargo build --release -p relay

# Copy to your server
scp target/release/relay your-server:/usr/local/bin/guess-up-relay
```

**Run it:**

```bash
# Simplest form (binds to 0.0.0.0:7878)
guess-up-relay

# Custom options
guess-up-relay --bind 0.0.0.0:9000 --max-rooms 50 --room-timeout 1800
```

| Flag | Default | Description |
|------|---------|-------------|
| `--bind` | `0.0.0.0:7878` | Address and port to listen on |
| `--max-rooms` | `100` | Max concurrent rooms |
| `--room-timeout` | `3600` | Seconds before an idle room is reaped |

**Open the firewall port:**

```bash
sudo ufw allow 7878/tcp
```

**Run as a systemd service (optional):**

Create `/etc/systemd/system/guess-up-relay.service`:

```ini
[Unit]
Description=Guess Up Relay Server
After=network.target

[Service]
ExecStart=/usr/local/bin/guess-up-relay --bind 0.0.0.0:7878
Restart=on-failure
User=nobody
Group=nogroup

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now guess-up-relay
```

**Check logs:**

```bash
# Direct — tracing output goes to stderr
RUST_LOG=info guess-up-relay

# Systemd
journalctl -u guess-up-relay -f
```

**Verify connectivity from a client machine:**

```bash
nc -zv your-server 7878
```

## Menu Navigation

All menus use the same controls:

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Enter` | Select / confirm |
| `Esc` / `q` | Go back / quit |
| `←` / `h` | Decrease value (settings) |
| `→` / `l` | Increase value (settings) |

In text input fields (server address, room code), type normally. `Enter` confirms, `Esc` cancels.

## Word List Format

The included list (`files/ASOIAF_list.txt`) has 420+ entries across 25 categories. Custom word files use the same format:

```
[Category Name]
Entry One
Entry Two

[Another Category]
Entry Three
```

Lines are trimmed and deduplicated automatically.

## Game Features

- **Interactive TUI menu** — configure all settings from the game, no CLI flags needed
- **Persistent settings** — saved to `~/.guess_up_config.json` between sessions
- **Single-keypress input** — `y`/`n`/`q` register instantly, no Enter required
- **Green/red flash** — visual feedback on correct/pass
- **Live timer and score** — updated every second
- **End-of-round summary** — score, accuracy %, pace, and missed words
- **Game history** — results saved to `~/.guess_up_history.json`
- **Category filtering** — scrollable picker with all 25 categories
- **Networked play** — two players on different machines via relay server
- **Role selection** — host picks Viewer or Holder, swap after each game
- **Post-game menu** — play again, swap roles, or quit (room stays alive)
- **Address validation** — relay addresses validated before connecting
- **Recent servers** — last 10 relay addresses remembered

## Architecture

Cargo workspace with three crates:

```
crates/
  protocol/   # shared message types + TCP framing
  relay/      # standalone relay server binary
  client/     # the game (solo + networked modes)
```

Channel-based async with tokio. Three concurrent tasks communicate via `tokio::sync::mpsc`:

```
Input Task (crossterm EventStream) ---> tx --+
                                             v
Timer Task (1s interval ticks)     ---> tx --> Game Loop (tokio::select!) --> Render
                                             ^
Network Task (TCP via relay)       ---> tx --+  (networked mode only)
```

| Module | Responsibility |
|--------|---------------|
| `main.rs` | Word loading, game runners (solo/host/join), entry point |
| `config.rs` | `AppConfig` — persistent settings, load/save `~/.guess_up_config.json` |
| `menu.rs` | TUI menu system — main menu, settings, server connect, room code screens |
| `types.rs` | Event types, game config, result structs |
| `game.rs` | Game state, main loop (solo + host), remote game loop |
| `input.rs` | Async single-keypress input via crossterm |
| `timer.rs` | 1-second interval ticks, bonus-time support |
| `render.rs` | Terminal guard (RAII cleanup), all rendering (game + lobby) |
| `net.rs` | TCP connection to relay, message translation |
| `lobby.rs` | Room setup, role selection, post-game flow |

## Roadmap

See [TODO.md](TODO.md) for planned features and improvements.

## License

MIT
