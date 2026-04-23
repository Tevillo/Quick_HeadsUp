# Architecture & Technical Reference

Everything under the hood of Guess Up — install layout, release packaging, networked mode, relay server setup, word lists, and the client architecture.

## Install Layout

The `guess_up` binary is self-contained and expects two siblings in its directory:

```
<install-dir>/
  guess_up                  # the binary
  lists/                    # one or more .txt word lists (required)
    ASOIAF_list.txt
  imports/                  # drop-in dir for the Import Word List flow (auto-created on first use)
  .history/                 # created automatically on first game
    history.json
  .guess_up_config.json     # created automatically on first save
```

Drop additional `.txt` files into `lists/` and they show up in the in-game **Word List** picker. To turn a CSV/TSV/JSON source into a `lists/`-compatible file without leaving the TUI, drop it into `imports/` and use **Settings → Import Word List** (see below).

When you run via `cargo run -p guess_up`, the build script copies the repo's `lists/` directory into `target/{debug,release}/` alongside the binary, so everything works out of the box. For release installs, copy the `guess_up` binary together with the `lists/` directory to wherever you want to run it. User config (`.guess_up_config.json`) is created next to the binary on first save.

If `guess_up` is launched without a controlling terminal (e.g. double-clicked from a file manager or a `.desktop` launcher), it detects this and re-launches itself inside a terminal emulator. On Linux it tries `$TERMINAL`, then `xdg-terminal-exec`, then a built-in fallback list (foot, alacritty, kitty, wezterm, gnome-terminal, konsole, xfce4-terminal, tilix, terminator, mate-terminal, lxterminal, xterm). On Windows it tries `wt.exe` then `cmd.exe /c start`. Pass `--no-spawn-terminal` to disable this. If no terminal can be found, a timestamped entry is appended to `.guess_up_launch_error.log` next to the binary.

## Release Packaging

A `Makefile` at the repo root builds distributable archives for Linux and Windows:

```bash
make release          # build all 4 archives
make release-linux    # Linux only
make release-windows  # Windows only
make help             # list all targets
```

Output lands in `./dist/`:

| Archive | Contents |
|---------|----------|
| `guess_up-<ver>-linux-x86_64.tar.gz`   | `guess_up` + `lists/` + `README.md` |
| `guess_up-<ver>-windows-x86_64.zip`    | `guess_up.exe` + `lists/` + `README.md` |
| `relay-<ver>-linux-x86_64.tar.gz`      | `relay` |
| `relay-<ver>-windows-x86_64.zip`       | `relay.exe` |

Requirements: the two rustup targets (`rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu`), `x86_64-w64-mingw32-gcc` for Windows cross-linking (configured in `.cargo/config.toml`), plus `tar` and `zip`.

## Networked Mode

Up to 9 players connect through a relay server. One player **hosts** a room (owns the game state, timer, and word list) and up to 8 others **join** with a room code.

### Hosting a Game

Select **Host Game** from the main menu, then enter your relay server address (e.g. `your-server:3000`). The host lobby shows the room code (an ASOIAF name like `HODOR` or `DROGON`), a live participant list, and lets you adjust **Settings** while waiting for players. Once at least one joiner connects, press **Start Game** and pick who will be the **Holder**:

- **Holder** — guesses based on clues and presses `y`/`n` (can be the host or any joiner)
- **Viewer** — everyone else sees the word on screen and gives verbal clues

After each game, the host sees the end-of-game stats (score, accuracy, pace, missed words) and post-game actions — play again (same holder), pick a new holder, or quit — in a single combined box inside the TUI. The room stays alive across games, so there's no need to reconnect.

### Joining a Game

Select **Join Game** from the main menu, enter the relay server address, then type the room code the host gave you (case doesn't matter — `hodor`, `HODOR`, and `Hodor` all work). If the code is wrong, the error appears inline so you can fix it and retry. After the game, the same stats box stays on screen with a "Waiting for host..." footer until the host kicks off the next round.

## Relay Server Setup

The relay is a lightweight TCP server that forwards messages between players. It knows nothing about game logic — all state lives on the host client. Rooms support up to 8 joiners plus the host.

**Version compatibility:** the client and relay must be built from the same workspace version. On connect the client sends a magic-byte + version handshake frame; the relay rejects anything else with an inline error ("wrong protocol magic" or "version mismatch: client X, relay Y"). Upgrade both sides together.

**Build and deploy:**

```bash
# Build
cargo build --release -p relay

# Copy to your server
scp target/release/relay your-server:/usr/local/bin/guess-up-relay
```

**Run it:**

```bash
# Simplest form (binds to 0.0.0.0:3000)
guess-up-relay

# Custom options
guess-up-relay --bind 0.0.0.0:9000 --max-rooms 50 --room-timeout 1800
```

| Flag | Default | Description |
|------|---------|-------------|
| `--bind` | `0.0.0.0:3000` | Address and port to listen on |
| `--max-rooms` | `100` | Max concurrent rooms |
| `--room-timeout` | `3600` | Seconds before an idle room is reaped |

**Open the firewall port:**

```bash
sudo ufw allow 3000/tcp
```

**Run as a systemd service (optional):**

Create `/etc/systemd/system/guess-up-relay.service`:

```ini
[Unit]
Description=Guess Up Relay Server
After=network.target

[Service]
ExecStart=/usr/local/bin/guess-up-relay --bind 0.0.0.0:3000
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
nc -zv your-server 3000
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

The included list (`lists/ASOIAF_list.txt`) has 420+ entries across 25 categories. Custom word files use the same format:

```
[Category Name]
Entry One
Entry Two

[Another Category]
Entry Three
```

Lines are trimmed and deduplicated automatically.

## Importing Word Lists

The TUI can convert external data files into the `lists/` format without leaving the game. Drop a source file into `imports/` (next to the binary — created automatically on first launch) and open **Settings → Import Word List**.

**Supported formats:**

| Format | Expected shape |
|--------|----------------|
| `.txt` | Newline-separated words; everything lands under a single `General` category |
| `.csv` / `.tsv` | One or more columns, first row is the header. Word column is auto-detected when the 2-column header is `word`, `name`, `entry`, or `term` (case-insensitive). Any layout that isn't auto-resolvable — 2 columns with unrecognized headers, or 3+ columns — prompts the user to pick the word column and then the category column. Fields may be wrapped in matched double quotes |
| `.json` | An object mapping category names to string arrays: `{ "Category A": ["word1", "word2"], "Category B": [...] }`. Any other JSON shape is rejected with a clean error |

**Flow:**

1. **Source picker** — pick a file from `imports/`. Every file is listed regardless of extension, so unrecognized formats still surface and produce a clear error at convert time rather than silently disappearing.
2. **Column pickers** — when the layout can't be auto-resolved, you pick the word column first, then the category column. The category picker always offers a **None — put everything under [General]** option, so a source with no category dimension (or one you'd rather flatten) still imports cleanly.
3. **Output filename** — defaults to the source stem with `.txt` appended. `.txt` is auto-appended if you omit it. Empty names and filenames containing `/`, `\`, or null bytes are rejected.
4. **Conflict resolution** — if the target already exists in `lists/`, pick **Overwrite**, **Use auto-suffix** (`<name>_1.txt`, `<name>_2.txt`, …), or **Cancel**.
5. **Result** — the converted list is written into `lists/` and you're shown an entry + category count. The new list appears in the existing Word List picker immediately; the active word list is never changed automatically.

Dedup is case-insensitive and applied globally across categories (first-occurrence wins), matching how `load_words` treats existing `.txt` lists. Empty categories and rows with missing word cells are silently dropped. Parsing is pure, in-memory, and unit-tested (`cargo test -p guess_up converter::`).

## Game Features

- **Interactive TUI menu** — configure all settings from the game, no CLI flags needed
- **Persistent settings** — saved to `.guess_up_config.json` next to the binary between sessions
- **Single-keypress input** — `y`/`n`/`q` register instantly, no Enter required
- **Green/red flash** — visual feedback on correct/pass
- **Live timer and score** — updated every second
- **End-of-round summary** — score, accuracy %, pace, and missed words shown inside the TUI for solo, host, and joiner (missed words truncate with `...and N more` when the list is long)
- **Game history** — results saved to `.history/history.json` in the install directory
- **Category filtering** — scrollable picker with all 25 categories
- **Color schemes** — 12 truecolor palettes (Classic, Pastel, Beige, and one for each of the nine ASOIAF great houses — Stark, Lannister, Tyrell, Martell, Greyjoy, Targaryen, Baratheon, Arryn, Tully). House Stark is the default. Pick one from **Settings → Color Scheme** — a live preview panel to the right of the list renders sample UI elements (menu, selected item, summary, error) in the hovered scheme's palette. Press Enter to keep it or Esc to cancel. Your terminal must support 24-bit color.
- **Multi-player rooms** — 1 host + up to 8 joiners via relay server
- **Holder selection** — host picks who holds the device from a participant list
- **Post-game menu** — play again, pick next holder, or quit (room stays alive)
- **Address validation** — relay addresses validated before connecting
- **Recent servers** — last 10 relay addresses remembered

## Client Architecture

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
| `main.rs` | Word loading, startup validation of `lists/`, game runners (solo/host/join), entry point |
| `config.rs` | `AppConfig` — persistent settings, load/save `.guess_up_config.json` next to the binary |
| `paths.rs` | Install-layout path resolution (binary dir, `lists/`, `imports/`, `.history/`) — single source of truth |
| `menu.rs` | TUI menu system — main menu, settings, word list picker, category picker, server connect, room code screens |
| `converter.rs` | Pure-logic word-list converter — CSV/TSV/JSON/plain-text parsers + deterministic emitter; fully unit-tested |
| `converter_menu.rs` | TUI flow for Settings → Import Word List (source picker, column picker, output name prompt, conflict resolution, result screen) |
| `list_menu.rs` | Shared `ListState` + `classify_key` helpers used by list-style screens (main menu, pickers, holder/post-game menus) |
| `types.rs` | Event types, game config, result structs |
| `game.rs` | Game state, main loop (solo + host), remote game loop |
| `input.rs` | Async single-keypress input via crossterm |
| `timer.rs` | 1-second interval ticks, bonus-time support |
| `render.rs` | Terminal guard (RAII cleanup), all rendering (game + lobby) |
| `net.rs` | TCP connection to relay, message translation, broadcast/targeted routing |
| `lobby.rs` | Room setup, multi-player lobby, holder selection, post-game flow |
| `terminal_spawn.rs` | Detect missing TTY and re-launch inside a terminal emulator (opt-out via `--no-spawn-terminal`) |
| `theme.rs` | Color scheme table (12 truecolor palettes) and active-scheme cell |

## Roadmap

See [TODO.md](TODO.md) for planned features and improvements.
