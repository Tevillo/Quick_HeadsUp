# TODO

| Status | Item | Difficulty | Release |
|--------|------|------------|---------|
| ✅ | Flash screen effect too short/unreliable — increase duration beyond 150ms (`crates/client/src/render.rs:108`) | Easy | v0.2 |
| ✅ | Rename all instances of "heads_up" / "Heads Up" to "guess_up" / "Guess Up" across codebase | Easy | v0.2 |
| ✅ | Fix timer skipping first second — timer often jumps from e.g. 10 to 8 on the first tick, and bonus time additions are inconsistent | Easy | v0.2 |
| ✅ | Fix viewer side screen flash — flash effect not displaying correctly on the joiner/viewer side in networked mode | Easy | v0.2 |
| ✅ | Universal Ctrl+C — allow Ctrl+C to break out of the terminal at any point during the game | Easy | v0.2 |
| ✅ | Rename config to `guess_up_config` — the configuration file should be named `guess_up_config` instead of the current name | Easy | v0.2 |
| ✅ | Return to lobby after game ends instead of exiting — show stats screen in-TUI, then back to menu | Medium | v0.2 |
| ✅ | Replace clap with TUI menu system ([plan](.claude/tui-menu-plan.md)) | Medium | v0.2 |
| ✅ | Self-contained install layout — ship `guess_up` so it runs from its own directory with two sibling dirs adjacent to the binary: a hidden `.history/` dir for game history (replacing `~/.guess_up_history.json`) and a `lists/` dir for word lists (replacing the hardcoded `files/ASOIAF_list.txt` path) | Medium | v0.2 |
| ✅ | Multi-viewer support — one host, up to 8 viewers in the same room ([plan](.claude/multi-viewer-plan.md)) | Hard | v0.2 |
| ✅ | ~~Spectator mode~~ — superseded by multi-viewer support | Hard | v0.2 |
| ✅ | Add color scheme option — starting schemes: pastel, beige, blue (shipped with 12 schemes: 3 generic + 9 ASOIAF great houses, truecolor) | Medium | v1.0 |
| ✅ | Show end-of-game stats in post-game lobby for all players (solo + networked) — score, words guessed/skipped visible to host and joiner inside the TUI; replaces the current post-exit print entirely | Medium | v1.0 |
| ✅ | Spawn a terminal when executable is run outside of one (e.g. double-clicked from file manager) | Medium | v1.0 |
| ✅ | Change default relay port from 7878 to 3000 — applies to both the relay's bind address and the client's default server address in `AppConfig` | Easy | v1.1 |
| ✅ | Change room code to a single ASOIAF name (<8 characters) — curate a hardcoded pool from `lists/ASOIAF_list.txt`; pool is assumed to outpace active room count (reroll on collision) | Easy | v1.1 |
| ✅ | One-way magic-bytes + crate-version handshake (client → relay) — client sends magic bytes + `CARGO_PKG_VERSION` as the first frame on connect; relay hard-rejects on wrong magic or version mismatch. No shared secret (protocol sanity only, not access control) | Medium | v1.1 |
| ✅ | Simplify menu code — extract a shared list-menu abstraction to eliminate duplicated up/down (↑/k, ↓/j) navigation and select/cancel handling across list-style screens in `menu.rs`. Text-input screens (server connect, join room) are excluded and keep their own pattern | Medium | v1.1 |
| ❌ | Low-time warning — visual cue when timer is running low (e.g. last 10s border turns red or timer text changes color) | Easy | v1.2 |
| ❌ | Custom word list support — allow users to create/import their own themed word lists beyond ASOIAF (partial: drop a `.txt` into `lists/` and it's auto-discovered; no in-app create/import UI yet) | Easy | v1.2 |
| ❌ | Round-based multiplayer — multiple rounds with automatic role swapping and cumulative scoring | Medium | v1.2 |
| ❌ | Investigate host-create failures on Windows — terminal goes blank and Ctrl+C is unresponsive when starting a Host; mostly reproduced via Explorer double-click. Solo works fine; also verify Join flow. Open-ended root-cause investigation (possible suspects: self-spawn into `wt.exe`/`cmd.exe`, blocking TCP connect, firewall prompt hidden behind window) | Medium | v1.2 |
| ✅ | Reorganize `lists/ASOIAF_list.txt` into broader top-level categories (fully replacing the current fine-grained ones like House Stark, House Lannister, Dragons, Valyrian Steel, etc.); exact category set left to judgment | Medium | v1.2 |
| ❌ | Server-side persistent stats — track date, games, scores, average, game type, slowest/fastest guess per session. Add player name system so matchup history (who played whom) is recorded. Historical trends viewable from client | Hard | v1.3 |
| ❌ | Dynamic word difficulty — calculate difficulty from historical data (guess time + skip rate relative to other words). Display word color based on difficulty. No filtering, just informational | Hard | v1.3 |
| ❌ | Unit-test layer for network/parsing plumbing — cover pure functions that silently regress under refactor: `protocol` framing + handshake, `relay::room_codes` pool/collision/uppercase behavior, `menu::normalize_address`, and word-list category parsing + dedup in `main.rs`. Manual play-testing stays the primary strategy for UI/flow | Medium | v1.4 |
| ❌ | Split `render.rs` (~927 lines) and `menu.rs` (~866 lines) along existing visual boundaries (game render vs menu render vs summary; main menu vs settings vs pickers vs lobby). Pure code-motion to keep diffs localized for upcoming v1.2 features — do before v1.2 work lands, not during | Medium | v1.4 |
| ❌ | Finish `.expect()` → `Result` migration called out in CLAUDE.md — single focused sweep across the client, then promote `cargo clippy -- -D warnings` and add `#![deny(clippy::unwrap_used)]` (or equivalent lint gate) so backslide is mechanical | Medium | v1.4 |
| ❌ | In-TUI word-list converter — import CSV, TSV, newline-separated, and JSON sources and emit a `lists/`-compatible `.txt` file. Inputs without a category key/column land under a `[General]` category. Ships with unit tests covering each input format and the "no separator → General" fallback. Pairs with the v1.2 "Custom word list support" UI | Medium | v1.2 |
| ❌ | General pre-v2.0 release-gate testing — one-time manual sweep of every scenario in CLAUDE.md's Testing section across Linux and Windows, for both `guess_up` and `relay`. Must be completed before cutting v2.0 | Medium | v2.0 |
| ❌ | Compile-time "configured" build of `guess_up` — baked relay address and baked room code, join-only menu flow (no server-connect, host, or solo screens); first client to connect to the empty baked room is implicitly promoted to host (requires relay/protocol support for implicit host promotion). Not part of the standard release — built from source via a cargo feature / build profile | Hard | v2.1 |
| ❌ | Menu flourishes — universal decorative accents on the outline of every bordered surface (menus, pickers, summary box, lobby) across all color schemes, plus house-sigil motifs shown on the game page only when a house-themed scheme is active. Unicode glyph compatibility across common terminals must be verified | Medium | v2.2 |
| ❌ | Reorganize Settings menu — it has grown long (10+ items after v1.2's Import Word List lands) and is approaching the visible-window cap. Group related settings into submenus (e.g. Game Rules, Appearance, Word Lists) or split into tabs. Keep navigation consistent with the existing `list_menu` helpers | Medium | v2.2 |
