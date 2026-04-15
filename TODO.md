# TODO

## Easy

- [ ] Flash screen effect too short/unreliable — increase duration beyond 150ms (`crates/client/src/render.rs:108`)
- [ ] Rename all instances of "heads_up" / "Heads Up" to "guess_up" / "Guess Up" across codebase
- [ ] Low-time warning — visual cue when timer is running low (e.g. last 10s border turns red or timer text changes color)
- [ ] Custom word list support — allow users to create/import their own themed word lists beyond ASOIAF

## Medium

- [ ] Add color scheme option — starting schemes: grayscale, pastel, beige, blue
- [ ] Return to lobby after game ends instead of exiting — show stats screen in-TUI, then back to menu
- [ ] Replace clap with TUI menu system ([plan](.claude/tui-menu-plan.md))
- [ ] Round-based multiplayer — multiple rounds with automatic role swapping and cumulative scoring
- [ ] Spawn a terminal when executable is run outside of one (e.g. double-clicked from file manager)

## Hard

- [ ] Multi-viewer support — one host, up to 8 viewers in the same room
- [ ] Server-side persistent stats — track date, games, scores, average, game type, slowest/fastest guess per session. Add player name system so matchup history (who played whom) is recorded. Historical trends viewable from client
- [ ] Dynamic word difficulty — calculate difficulty from historical data (guess time + skip rate relative to other words). Display word color based on difficulty. No filtering, just informational
- [ ] Spectator mode — read-only viewers who can watch a networked game in progress

## Completed

- [x] Rewrite game with proper async architecture ([plan](.claude/claude-plan.md))
- [x] Add networked P2P mode via relay server ([plan](.claude/claude-p2p-plan.md))
