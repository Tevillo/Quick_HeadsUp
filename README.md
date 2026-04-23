# Guess Up! — ASOIAF Edition

A terminal-based "Heads Up" party game, themed around *A Song of Ice and Fire*. 

Play solo with friends in the same room, or get up to 9 people across the internet into the same game through a relay server (1 host + 8 joiners).

## Quickstart

Grab a prebuilt binary from the **[latest release](https://github.com/Tevillo/Guess-Up/releases/latest)** — Linux and Windows archives are available. Simply unpack and double click `guess_up` (or `guess_up.exe`), and you're in. Configure settings, color schemes, and lists from inside the application! 

Press `q` any time to quit. The terminal always restores cleanly, even on Ctrl+C.

For networked play the `guess_up` client and the `relay` server must be built from the same crate version. The client sends a version handshake on connect; the relay rejects any mismatch with an inline error, so both sides need to be upgraded together.

## Importing Your Own Word Lists

Drop a source file into the `imports/` directory next to the binary (created automatically on first launch) and run **Settings → Import Word List**. Supported formats: `.csv`, `.tsv`, `.json` (shape `{ "Category": ["word", ...] }`), and plain newline-separated `.txt`. 1-column sources land under a single `General` category; 2-column CSV/TSV pick the word column automatically when the header is `word`/`name`/`entry`/`term`. If the layout isn't auto-resolvable — 2 columns with unrecognized headers, or 3+ columns — you pick the word column and then the category column (with a **None → [General]** option). The converted list lands in `lists/` and shows up in the Word List picker immediately — see [ARCHITECTURE.md — Importing Word Lists](ARCHITECTURE.md#importing-word-lists) for the full flow.

---
### Build From Source

If you'd rather build it yourself, you'll need [Rust](https://www.rust-lang.org/tools/install) (any recent stable toolchain — `rustup toolchain install stable` if you hit a version issue).

```bash
# Build everything (client + relay server)
cargo build --release

# Launch the game
cargo run -p guess_up
```

## Technical Details

For the full tour — install layout, release packaging, networked play, relay server setup, word list format, menu controls, and client architecture — see **[ARCHITECTURE.md](ARCHITECTURE.md)**.

## License

MIT
