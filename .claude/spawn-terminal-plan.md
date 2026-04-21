# Spawn Terminal When Launched Without One

## Goal

When `guess_up` is executed without a controlling terminal (e.g. double-clicked from a file manager, launched from a `.desktop` file without `Terminal=true`), detect the situation, re-launch the binary inside a terminal emulator, and let the original process exit. If no terminal can be found, write a log file next to the binary so the user has a breadcrumb to follow.

## Scope (from clarifying questions)

- **Platforms**: Linux + Windows. No macOS for now.
- **Linux picker**: Auto-detect, no config. Priority order: `$TERMINAL` → `xdg-terminal-exec` → built-in fallback list.
- **Fallback on total failure**: write `<install_dir>/.guess_up_launch_error.log` and exit 1.
- **Opt-out**: `--no-spawn-terminal` CLI flag.

Non-goals: GUI error dialogs, config-driven terminal choice, `.desktop` file shipping, `GUESS_UP_NO_SPAWN` env var.

## Design

### Detection

Use `std::io::IsTerminal` (stable since 1.70, no new deps):

```rust
!std::io::stdout().is_terminal() && !std::io::stdin().is_terminal()
```

Both must be non-tty. This avoids spawning when the user intentionally pipes stdout to a file (`./guess_up > log.txt` keeps stdin as tty).

### Loop prevention

Set `GUESS_UP_SPAWNED=1` on the child process. The child's first action is to check this env var and skip the spawn logic even if its TTY detection is somehow wrong.

### Linux: terminal selection

Try in order, first success wins:

1. **`$TERMINAL`** — user's explicit preference, invoked as `$TERMINAL -e <cmd>` (most common convention).
2. **`xdg-terminal-exec`** — XDG freedesktop standard. Invoked as `xdg-terminal-exec <cmd>`.
3. **Built-in fallback list**, first found on `$PATH`:
   - `foot`, `alacritty`, `kitty`, `wezterm`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `tilix`, `terminator`, `mate-terminal`, `lxterminal`, `xterm`

Each entry in the fallback list has its own arg-builder because flag syntax varies:

| Terminal | Invocation |
|----------|------------|
| `foot`, `kitty`, `alacritty` | `<term> <bin> [args]` |
| `wezterm` | `wezterm start -- <bin> [args]` |
| `gnome-terminal` | `gnome-terminal -- <bin> [args]` |
| `konsole`, `xfce4-terminal`, `tilix`, `xterm`, `lxterminal`, `mate-terminal` | `<term> -e <bin> [args]` |
| `terminator` | `terminator -x <bin> [args]` |

Using `which` via `std::process::Command::new(...).spawn()` with error capture tells us if the binary exists — no need for a `which` dep. Simpler: attempt `spawn()`, on `ErrorKind::NotFound` move to the next candidate.

### Windows

Rust binaries default to the console subsystem, so double-clicking a `.exe` in Explorer auto-allocates a console — `is_terminal()` returns true, and the spawn path is never taken. The Windows code path therefore mostly exists as defense-in-depth (e.g. launched with stdin/stdout redirected by another program).

Logic when detection fires on Windows:

1. `wt.exe` (Windows Terminal, default on Win11) — `wt.exe new-tab <bin> [args]`.
2. `cmd.exe /c start <bin> [args]` as a last resort.

### Spawning

- Resolve current binary: `std::env::current_exe()?`.
- Forward the original args (excluding `--no-spawn-terminal`, though we won't have that since detection implies we're not in a shell that passed it).
- Pass `GUESS_UP_SPAWNED=1` as the only extra env var.
- Use `Command::spawn()` (not `exec`) so the parent returns immediately.
- On success, parent exits 0 (the child owns the session now).

### Error logging

On total failure (no terminal found):

- Path: `<install_dir>/.guess_up_launch_error.log`
- Format: `[RFC3339 timestamp] terminal spawn failed: <reason>\n`
- Mode: append. Keep a short history if the user retries.
- Best-effort — if log write itself fails, exit silently (we already have no UI).

### CLI flag

Minimal argv parse in `main.rs`, no clap dep:

```rust
let skip_spawn = std::env::args().any(|a| a == "--no-spawn-terminal");
```

Placed before the spawn check.

## Module layout

New file: `crates/client/src/terminal_spawn.rs`

```rust
pub enum SpawnOutcome {
    ShouldContinue,  // we're in a TTY, run game normally
    Spawned,         // child launched, parent should exit 0
    Failed,          // logged to file, parent should exit 1
}

pub fn spawn_if_needed(skip: bool) -> SpawnOutcome;
```

Internals:
- `fn has_tty() -> bool`
- `fn already_spawned() -> bool` (checks `GUESS_UP_SPAWNED`)
- `fn log_error(msg: &str)` → writes to `<install_dir>/.guess_up_launch_error.log`
- `#[cfg(unix)] fn try_spawn_linux(...) -> io::Result<()>`
- `#[cfg(windows)] fn try_spawn_windows(...) -> io::Result<()>`

## Integration point

In `crates/client/src/main.rs`, very first lines of `async fn main()`:

```rust
let skip_spawn = std::env::args().any(|a| a == "--no-spawn-terminal");
match terminal_spawn::spawn_if_needed(skip_spawn) {
    SpawnOutcome::ShouldContinue => {}
    SpawnOutcome::Spawned => return,
    SpawnOutcome::Failed => std::process::exit(1),
}
```

Also register `mod terminal_spawn;` at the top of `main.rs` alongside the other modules.

## Docs updates (before opening PR)

- `README.md`: one-liner under Usage that the binary self-spawns a terminal when launched from a file manager; document `--no-spawn-terminal`.
- `CLAUDE.md`: add `terminal_spawn.rs` to the module table; note the `GUESS_UP_SPAWNED` sentinel env var under Key Implementation Details.
- `TODO.md`: move the checkbox to `[x]` under Medium / Next Release.

## Manual test plan

Linux (primary):

1. `cargo build --release`, then `./target/release/guess_up` from a normal shell → plays as usual (TTY present, no spawn).
2. `setsid ./target/release/guess_up < /dev/null > /dev/null 2>&1 &` (detached, no TTY) → a new terminal window opens running the game.
3. `./target/release/guess_up --no-spawn-terminal < /dev/null > /dev/null` → exits without spawning (the game will fail to render, which is the expected behavior for the escape hatch).
4. With `$TERMINAL=xterm` set → spawn picks xterm.
5. Temporarily rename all fallback terminal binaries in `$PATH` (or test inside a stripped Docker container) → `.guess_up_launch_error.log` appears next to the binary with a timestamped entry.
6. Double-click `guess_up` in a file manager (nautilus/dolphin/thunar) → terminal pops up with the game running.

Windows (secondary, opportunistic):

7. Double-click from Explorer → a console window opens (default OS behavior, spawn code path not reached).
8. If test environment allows: launch with stdin/stdout redirected such that `is_terminal()` returns false → `wt.exe` opens.

## Risks / open questions

- **Fork-bomb safety**: relying on `GUESS_UP_SPAWNED=1` is adequate; if the child somehow also fails the TTY check, it won't re-spawn.
- **Argument forwarding edge cases**: if the user ever adds args with spaces or special characters (not currently the case since there are no CLI args beyond `--no-spawn-terminal`), we'd need proper quoting per terminal. Ignoring for now.
- **`xdg-terminal-exec` adoption**: newish, not ubiquitous. That's fine — it's just step 2 of 3.
- **Wayland-only setups with no X fallback**: `xterm` would fail; modern list members (foot, alacritty) should cover it.
- **SSH sessions**: `is_terminal()` is true over SSH, no spawn attempted. Correct behavior.

## Branch & PR

- Feature branch: `spawn-terminal-when-detached`
- Single PR into `main` with docs updates included in the same branch (per CLAUDE.md).
