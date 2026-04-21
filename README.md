# Guess Up! — ASOIAF Edition

A terminal-based "Guess Up" party game, themed around A Song of Ice and Fire. Hold the screen up to your forehead, let your friends shout clues, and press `y` for correct or `n` to pass before the timer runs out. No mouse, no Enter key — just fast, chaotic fun.

Play solo with a pile of friends in the same room, or get up to 9 people across the internet into the same game through a relay server (1 host + 8 joiners).

## Download

Grab a prebuilt binary from the **[latest release](https://github.com/Tevillo/Guess-Up/releases/latest)** — Linux and Windows archives are available. Unpack, run `guess_up` (or `guess_up.exe`), and you're in.

## Build From Source

If you'd rather build it yourself, you'll need [Rust](https://www.rust-lang.org/tools/install) (any recent stable toolchain — `rustup toolchain install stable` if you hit a version issue).

```bash
# Build everything (client + relay server)
cargo build --release

# Launch the game
cargo run -p guess_up
```

That's it. An interactive TUI menu takes over from there — pick a word list, tweak the timer, choose a color scheme, and start playing. Your settings stick around between runs in `.guess_up_config.json` next to the binary.

Press `q` any time to quit. The terminal always restores cleanly, even on Ctrl+C.

## Technical Details

For the full tour — install layout, release packaging, networked play, relay server setup, word list format, menu controls, and client architecture — see **[ARCHITECTURE.md](ARCHITECTURE.md)**.

## License

MIT
