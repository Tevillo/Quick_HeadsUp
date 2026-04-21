# TODO

## Next Release

Items required before cutting the next release (from triage on 2026-04-21):

- [x] Add color scheme option — starting schemes: grayscale, pastel, beige, blue
- [ ] Show end-of-game stats in post-game lobby for all players (solo + networked) — score, words guessed/skipped visible to host and joiner inside the TUI; replaces the current post-exit print entirely
- [x] Spawn a terminal when executable is run outside of one (e.g. double-clicked from file manager)

Deferred to a later release: low-time warning, viewer-side screen flash fix, custom word list support (in-app create/import UI), round-based multiplayer, server-side persistent stats, dynamic word difficulty.

## Easy

- [x] Flash screen effect too short/unreliable — increase duration beyond 150ms (`crates/client/src/render.rs:108`)
- [x] Rename all instances of "heads_up" / "Heads Up" to "guess_up" / "Guess Up" across codebase
- [ ] Low-time warning — visual cue when timer is running low (e.g. last 10s border turns red or timer text changes color)
- [x] Fix timer skipping first second — timer often jumps from e.g. 10 to 8 on the first tick, and bonus time additions are inconsistent
- [ ] Fix viewer side screen flash — flash effect not displaying correctly on the joiner/viewer side in networked mode
- [ ] Custom word list support — allow users to create/import their own themed word lists beyond ASOIAF (partial: drop a `.txt` into `lists/` and it's auto-discovered; no in-app create/import UI yet)
- [x] Universal Ctrl+C — allow Ctrl+C to break out of the terminal at any point during the game
- [x] Rename config to `guess_up_config` — the configuration file should be named `guess_up_config` instead of the current name

## Medium

- [x] Add color scheme option — starting schemes: pastel, beige, blue (shipped with 12 schemes: 3 generic + 9 ASOIAF great houses, truecolor)
- [x] Return to lobby after game ends instead of exiting — show stats screen in-TUI, then back to menu
- [x] Replace clap with TUI menu system ([plan](.claude/tui-menu-plan.md))
- [ ] Show end-of-game stats in post-game lobby for all players (solo + networked) — score, words guessed/skipped visible to host and joiner inside the TUI; replaces the current post-exit print entirely
- [ ] Round-based multiplayer — multiple rounds with automatic role swapping and cumulative scoring
- [x] Spawn a terminal when executable is run outside of one (e.g. double-clicked from file manager)
- [x] Self-contained install layout — ship `guess_up` so it runs from its own directory with two sibling dirs adjacent to the binary: a hidden `.history/` dir for game history (replacing `~/.guess_up_history.json`) and a `lists/` dir for word lists (replacing the hardcoded `files/ASOIAF_list.txt` path)

## Hard

- [x] Multi-viewer support — one host, up to 8 viewers in the same room ([plan](.claude/multi-viewer-plan.md))
- [ ] Server-side persistent stats — track date, games, scores, average, game type, slowest/fastest guess per session. Add player name system so matchup history (who played whom) is recorded. Historical trends viewable from client
- [ ] Dynamic word difficulty — calculate difficulty from historical data (guess time + skip rate relative to other words). Display word color based on difficulty. No filtering, just informational
- [ ] ~~Spectator mode~~ — superseded by multi-viewer support

## Completed

- [x] Rewrite game with proper async architecture ([plan](.claude/claude-plan.md))
- [x] Add networked P2P mode via relay server ([plan](.claude/claude-p2p-plan.md))
