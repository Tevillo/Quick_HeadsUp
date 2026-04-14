# Heads Up! - ASOIAF Edition

A terminal-based "Heads Up" party game themed around A Song of Ice and Fire. Play solo (hold the screen to your forehead while friends give clues) or networked (two players on different machines connected through a relay server).

Press `y` for correct, `n` to pass — no Enter needed.

## Quickstart

Requires [Rust](https://www.rust-lang.org/tools/install). If you run into version issues, run `rustup toolchain install stable`.

```bash
# Build everything (client + relay server)
cargo build --release

# Play solo — default 60-second game
cargo run -p heads_up

# Or play networked (see sections below)
cargo run -p heads_up -- host --relay your-server:7878
cargo run -p heads_up -- join --relay your-server:7878 --code ABCDE
```

Press `q` at any time to quit. The terminal always restores cleanly, even on Ctrl+C.

## Solo Mode

When you run without a subcommand, you get the original single-device experience:

```bash
cargo run -p heads_up                              # default 60-second game
cargo run -p heads_up -- -g 90                     # 90-second game
cargo run -p heads_up -- -s                        # skip the 3-2-1 countdown
cargo run -p heads_up -- -l                        # unlimited time on the last question
cargo run -p heads_up -- -x                        # extra-time mode (correct answers add time)
cargo run -p heads_up -- -x --bonus-seconds 3      # add 3s per correct answer
cargo run -p heads_up -- --category "House Stark"  # only House Stark entries
cargo run -p heads_up -- -w my_words.txt           # use a custom word list
```

## Networked Mode

Two players connect through a relay server. One player **hosts** a room (owns the game state, timer, and word list) and the other **joins** with a room code. After connecting, the host picks roles:

- **Viewer** — sees the word on screen and gives verbal clues
- **Holder** — guesses based on clues and presses `y`/`n`

After each game, both players get a post-game menu to play again, swap roles, or quit.

### Hosting a Game

```bash
cargo run -p heads_up -- host --relay your-server:7878
```

This connects to the relay, creates a room, and displays a 5-letter room code (e.g. `STARK`). Share this code with your opponent. Once they join, you'll pick your role and the game starts.

All solo-mode flags work with `host` too:

```bash
cargo run -p heads_up -- -g 90 -x --bonus-seconds 3 host --relay your-server:7878
```

The host controls the game config — the joiner receives it automatically.

### Joining a Game

```bash
cargo run -p heads_up -- join --relay your-server:7878 --code STARK
```

Enter the room code the host gave you (case-insensitive). You'll see your assigned role, then the game starts. The joiner doesn't need to specify game flags — the host's settings are used.

### Relay Server Setup

The relay is a lightweight TCP server that forwards messages between the two players. It knows nothing about game logic — all state lives on the host client.

**Build and deploy:**

```bash
# Build
cargo build --release -p relay

# Copy to your server
scp target/release/relay your-server:/usr/local/bin/heads-up-relay
```

**Run it:**

```bash
# Simplest form (binds to 0.0.0.0:7878)
heads-up-relay

# Custom options
heads-up-relay --bind 0.0.0.0:9000 --max-rooms 50 --room-timeout 1800
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

Create `/etc/systemd/system/heads-up-relay.service`:

```ini
[Unit]
Description=Heads Up Relay Server
After=network.target

[Service]
ExecStart=/usr/local/bin/heads-up-relay --bind 0.0.0.0:7878
Restart=on-failure
User=nobody
Group=nogroup

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now heads-up-relay
```

**Check logs:**

```bash
# Direct — tracing output goes to stderr
RUST_LOG=info heads-up-relay

# Systemd
journalctl -u heads-up-relay -f
```

**Verify connectivity from a client machine:**

```bash
nc -zv your-server 7878
```

## CLI Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-g, --game-time <seconds>` | 60 | Game length in seconds |
| `-s, --skip-countdown` | off | Skip the 3-2-1 countdown |
| `-l, --last-unlimited` | off | Give unlimited time on the final question |
| `-x, --extra-time` | off | Correct answers add bonus time |
| `--bonus-seconds <n>` | 5 | Seconds added per correct answer (with `-x`) |
| `-w, --word-file <path>` | `files/ASOIAF_list.txt` | Path to a word list file |
| `--category <name>` | all | Filter to a specific category |

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

- **Single-keypress input** — `y`/`n`/`q` register instantly, no Enter required
- **Green/red flash** — visual feedback on correct/pass
- **Live timer and score** — updated every second
- **End-of-round summary** — score, accuracy %, pace, and missed words
- **Game history** — results saved to `~/.heads_up_history.json`
- **Category filtering** — play with only the entries you want
- **Networked play** — two players on different machines via relay server
- **Role selection** — host picks Viewer or Holder, swap after each game
- **Post-game menu** — play again, swap roles, or quit

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
| `main.rs` | CLI args, word loading, channel setup, task spawning |
| `types.rs` | Event types, game config, result structs |
| `game.rs` | Game state, main loop (solo + host), remote game loop |
| `input.rs` | Async single-keypress input via crossterm |
| `timer.rs` | 1-second interval ticks, bonus-time support |
| `render.rs` | Terminal guard (RAII cleanup), all rendering (game + lobby) |
| `net.rs` | TCP connection to relay, message translation |
| `lobby.rs` | Room setup, role selection, post-game flow |

## License

MIT
