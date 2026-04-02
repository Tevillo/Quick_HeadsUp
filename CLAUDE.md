# CLAUDE.md — Heads Up CLI Game

## What This Is

An ASOIAF (A Song of Ice and Fire) themed "Heads Up" party game for the terminal, built in Rust. Players see a name/term on screen and press `y` (correct) or `n` (pass) before the timer runs out.

## Build & Run

```bash
cargo build --release        # optimized binary at target/release/heads_up
cargo run                    # dev build, default 60s game
cargo run -- --extra-time --bonus-seconds 3   # extra-time mode
cargo run -- --category "House Stark"          # filter by category
```

CLI flags: `-g` (game time), `-s` (skip countdown), `-l` (last unlimited), `-x` (extra time), `--bonus-seconds`, `-w` (word file), `--category`.

## Architecture

Channel-based async with tokio. Three concurrent tasks communicate via `tokio::sync::mpsc`:

```
Input Task (crossterm EventStream) ---> tx --+
                                             v
Timer Task (1s interval ticks)     ---> tx --> Game Loop (tokio::select!) --> Render
```

This architecture is intentional — it cleanly separates input, timing, and game logic, and is designed to enable a future **networked mode** where the input task is swapped for a network listener without touching game logic.

### Modules

| Module | Responsibility |
|--------|---------------|
| `main.rs` | CLI args (clap), word loading with category parsing, channel setup, task spawning |
| `types.rs` | `GameEvent`, `UserAction`, `GameMode`, `GameConfig`, `GameResult`, type aliases |
| `game.rs` | `GameState`, game loop (`tokio::select!`), Fisher-Yates shuffle, history saving |
| `input.rs` | `input_task()` — crossterm `EventStream`, single-keypress in raw mode |
| `timer.rs` | `timer_task()` — 1s interval ticks, bonus-time channel for extra-time mode |
| `render.rs` | `TerminalGuard` (RAII cleanup), rendering, flash, countdown, summary output |

This split will evolve as features are added (e.g., networking, persistence modules). The key invariant is that `input.rs` stays separate from `game.rs` to allow swapping input sources.

### Key Implementation Details

- **TerminalGuard**: RAII pattern with `Drop` — ensures raw mode and alternate screen are cleaned up on panic or early exit.
- **Flash race condition**: `flash_screen()` clobbers the display; after 150ms it sends `GameEvent::Redraw` so the game loop re-renders the current word.
- **Summary rendering**: `TerminalGuard` must be dropped *before* `print_output()` — otherwise the summary prints inside the alternate screen buffer and gets wiped.
- **Word loading**: Parses `[Category]` headers, trims lines, deduplicates via `HashSet` (case-insensitive).
- **History**: Saved to `~/.heads_up_history.json` via serde.

## Coding Standards

- **Formatting**: `cargo fmt` with default settings.
- **Linting**: `cargo clippy` — all warnings should be clean.
- **Error handling**: Prefer `Result` propagation with `?` over `.expect()` / `.unwrap()`. Existing `.expect()` calls are legacy and should migrate toward `Result`-based handling over time.
- **Style**: Idiomatic Rust — `match` expressions, iterators, `Option`/`Result` types. Snake_case functions, PascalCase types.

## Testing

Manual play-testing is the primary test strategy. Key scenarios to verify:

1. `cargo run` — normal mode works end-to-end
2. `cargo run -- --last-unlimited` — last question gets infinite time
3. `cargo run -- --extra-time --bonus-seconds 3` — bonus time adds correctly
4. `cargo run -- --category "House Stark"` — category filtering works
5. Ctrl+C during game — terminal restores cleanly
6. `~/.heads_up_history.json` is written after a game

## Working With Claude

- **Ask before implementing**: Before making changes, ask pertinent clarifying questions to ensure alignment on scope, approach, and intent.

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
- Branch naming: `flashing-lights` or descriptive feature names.
- Branch names should be in kebab case.
- **All changes go through PRs. Direct pushes to `main` are forbidden.**

## Future Plans

- **Networked mode**: Two machines — one sees timer/scores ("viewer"), the other shows names and captures input ("holder"). Replace `input_task` with TCP/WebSocket listener.
- **Game history CLI viewer**: View `~/.heads_up_history.json` from the command line.
- **Multi-round / replay**: "Play again?" prompt after a round.
- **Configurable key bindings**: Currently hardcoded to y/n/q.
- **README update**: README still reflects v0.1.

## Word List

`files/ASOIAF_list.txt` — 420 entries across 25 categories (House Stark, Dragons, Valyrian Steel, Theories, etc.). Format:

```
[Category Name]
Entry One
Entry Two
```
