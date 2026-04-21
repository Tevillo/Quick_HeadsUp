# Color Schemes Plan

Add a user-selectable color scheme to the TUI. 13 schemes total: 4 generic + 9 ASOIAF great houses. Truecolor (RGB) throughout.

Branch: `color-schemes`.

---

## 1. Scheme Slots

Each scheme defines a palette of named slots. Every `SetColors(...)` call in `render.rs` will resolve to slots on the active scheme rather than hardcoded crossterm names.

| Slot | Used by | Today's "Blue" value |
|---|---|---|
| `primary_bg` | menu / pickers / question screen / lobby / holder view | `Blue` |
| `primary_fg` | default text on primary_bg | `White` |
| `accent_fg` | title text, selected item text | `DarkYellow` |
| `selection_bg` | bg behind selected menu item | `Black` |
| `error_bg` | error panel bg (`render_error`) | `Red` |
| `summary_bg` | post-game summary panel bg | `Black` |
| `summary_border` | summary box border text color | `Blue` |
| `summary_accent` | summary stat lines, "Missed words" header | `DarkYellow` |
| `summary_success` | "You cleared the entire list!" | `Green` |

**Holder view uses `primary_bg` / `primary_fg`** (aligned with viewer — dropped the separate `holder_bg` slot per your call).

**Flash correct/pass stay hardcoded `Green`/`Red`** across all schemes.

---

## 2. Palette Proposals

All values are `(r, g, b)`. Redline any that read wrong; I'll adjust in one pass before coding.

### Generic

**Blue** (preserves current look)
- primary_bg `(0, 0, 170)`, primary_fg `(255, 255, 255)`
- accent_fg `(170, 170, 0)`, selection_bg `(0, 0, 0)`
- error_bg `(170, 0, 0)`
- summary_bg `(0, 0, 0)`, summary_border `(0, 0, 170)`, summary_accent `(170, 170, 0)`, summary_success `(0, 170, 0)`

**Grayscale**
- primary_bg `(40, 40, 40)`, primary_fg `(220, 220, 220)`
- accent_fg `(255, 255, 255)`, selection_bg `(80, 80, 80)`
- error_bg `(110, 110, 110)`
- summary_bg `(0, 0, 0)`, summary_border `(180, 180, 180)`, summary_accent `(255, 255, 255)`, summary_success `(200, 200, 200)`

**Pastel** (inverts polarity — light bg, dark fg)
- primary_bg `(220, 210, 240)`, primary_fg `(60, 50, 90)`
- accent_fg `(200, 120, 140)`, selection_bg `(255, 235, 215)`
- error_bg `(240, 180, 180)`
- summary_bg `(240, 235, 250)`, summary_border `(160, 140, 200)`, summary_accent `(200, 120, 140)`, summary_success `(150, 200, 160)`

**Beige**
- primary_bg `(210, 180, 140)`, primary_fg `(60, 40, 20)`
- accent_fg `(180, 90, 40)`, selection_bg `(240, 220, 180)`
- error_bg `(180, 70, 50)`
- summary_bg `(40, 30, 20)`, summary_border `(180, 140, 90)`, summary_accent `(220, 160, 80)`, summary_success `(150, 160, 90)`

### Houses

**Stark** — slate grey + frost white (Winter Is Coming)
- primary_bg `(50, 55, 60)`, primary_fg `(230, 235, 240)`
- accent_fg `(180, 190, 200)`, selection_bg `(90, 95, 100)`
- error_bg `(100, 40, 40)`
- summary_bg `(20, 25, 30)`, summary_border `(140, 150, 160)`, summary_accent `(220, 220, 230)`, summary_success `(160, 200, 180)`

**Lannister** — crimson + gold (Hear Me Roar)
- primary_bg `(130, 20, 30)`, primary_fg `(255, 220, 100)`
- accent_fg `(255, 240, 180)`, selection_bg `(90, 10, 20)`
- error_bg `(60, 10, 10)`
- summary_bg `(30, 5, 10)`, summary_border `(180, 40, 40)`, summary_accent `(255, 220, 100)`, summary_success `(220, 180, 80)`

**Tyrell** — forest green + straw gold (Growing Strong)
- primary_bg `(30, 90, 50)`, primary_fg `(255, 230, 140)`
- accent_fg `(255, 200, 210)`, selection_bg `(80, 130, 70)`
- error_bg `(130, 60, 30)`
- summary_bg `(15, 40, 25)`, summary_border `(100, 170, 100)`, summary_accent `(255, 220, 130)`, summary_success `(200, 240, 160)`

**Martell** — sunset orange + spear red (Unbowed, Unbent, Unbroken)
- primary_bg `(200, 80, 20)`, primary_fg `(255, 230, 160)`
- accent_fg `(255, 80, 60)`, selection_bg `(140, 50, 10)`
- error_bg `(120, 20, 20)`
- summary_bg `(60, 20, 10)`, summary_border `(220, 120, 50)`, summary_accent `(255, 200, 100)`, summary_success `(240, 220, 120)`

**Greyjoy** — kraken black + gold (We Do Not Sow)
- primary_bg `(15, 15, 20)`, primary_fg `(200, 170, 80)`
- accent_fg `(100, 140, 140)`, selection_bg `(40, 45, 55)`
- error_bg `(80, 30, 40)`
- summary_bg `(5, 5, 10)`, summary_border `(60, 100, 110)`, summary_accent `(220, 180, 80)`, summary_success `(120, 180, 160)`

**Targaryen** — black + dragonfire red (Fire and Blood)
- primary_bg `(10, 10, 10)`, primary_fg `(220, 40, 40)`
- accent_fg `(255, 140, 60)`, selection_bg `(60, 10, 10)`
- error_bg `(140, 20, 20)`
- summary_bg `(5, 0, 0)`, summary_border `(180, 40, 40)`, summary_accent `(255, 160, 60)`, summary_success `(200, 160, 60)`

**Baratheon** — stag gold + black (Ours Is the Fury)
- primary_bg `(200, 160, 40)`, primary_fg `(20, 20, 20)`
- accent_fg `(80, 20, 20)`, selection_bg `(255, 210, 80)`
- error_bg `(120, 30, 30)`
- summary_bg `(20, 20, 20)`, summary_border `(200, 160, 40)`, summary_accent `(120, 30, 30)`, summary_success `(100, 140, 50)`

**Arryn** — sky blue + white (As High as Honor)
- primary_bg `(100, 140, 200)`, primary_fg `(245, 245, 250)`
- accent_fg `(220, 230, 240)`, selection_bg `(60, 90, 160)`
- error_bg `(120, 40, 40)`
- summary_bg `(20, 30, 50)`, summary_border `(130, 170, 220)`, summary_accent `(240, 240, 250)`, summary_success `(200, 220, 240)`

**Tully** — river blue + silver + Tully red (Family, Duty, Honor)
- primary_bg `(40, 60, 110)`, primary_fg `(220, 220, 230)`
- accent_fg `(200, 60, 60)`, selection_bg `(90, 30, 40)`
- error_bg `(140, 30, 30)`
- summary_bg `(20, 30, 50)`, summary_border `(100, 130, 180)`, summary_accent `(220, 80, 80)`, summary_success `(180, 200, 220)`

---

## 3. Data Model

New module `crates/client/src/theme.rs`:

```rust
use crossterm::style::Color;

pub struct ColorScheme {
    pub name: &'static str,       // display name, e.g. "House Stark"
    pub id: &'static str,         // persistence key, e.g. "stark"
    pub primary_bg: Color,
    pub primary_fg: Color,
    pub accent_fg: Color,
    pub selection_bg: Color,
    pub error_bg: Color,
    pub summary_bg: Color,
    pub summary_border: Color,
    pub summary_accent: Color,
    pub summary_success: Color,
}

pub const SCHEMES: &[ColorScheme] = &[ /* 13 entries, order = menu order */ ];

pub fn by_id(id: &str) -> &'static ColorScheme { /* lookup by id, default → Blue */ }
pub fn default_scheme() -> &'static ColorScheme { /* "blue" */ }
```

- `Color::Rgb { r, g, b }` values everywhere — no ANSI named colors in the scheme table.
- `SCHEMES` order defines picker order: Blue, Grayscale, Pastel, Beige, then houses in the order given above.
- Scheme selection is looked up per-render from `ActiveScheme`, a `once_cell::sync::OnceCell` (or `std::sync::OnceLock`) holding `&'static ColorScheme`. Set once at startup from `AppConfig`, updated when the user changes the setting.

---

## 4. Render Changes

Every `SetColors(Colors::new(...))` call in `render.rs` becomes a lookup on the active scheme. Concretely, add helpers:

```rust
fn fg_on_primary() -> Colors { Colors::new(scheme().primary_fg, scheme().primary_bg) }
fn accent_on_primary() -> Colors { Colors::new(scheme().accent_fg, scheme().primary_bg) }
fn accent_on_selection() -> Colors { Colors::new(scheme().accent_fg, scheme().selection_bg) }
fn error_panel() -> Colors { Colors::new(scheme().primary_fg, scheme().error_bg) }
fn summary_border() -> Colors { Colors::new(scheme().summary_border, scheme().summary_bg) }
fn summary_accent() -> Colors { Colors::new(scheme().summary_accent, scheme().summary_bg) }
fn summary_success() -> Colors { Colors::new(scheme().summary_success, scheme().summary_bg) }
```

### Call-site mapping in `render.rs`

| Current | New |
|---|---|
| `White, Blue` (menu/question/lobby/holder bg) | `fg_on_primary()` |
| `DarkYellow, Blue` (title on primary) | `accent_on_primary()` |
| `DarkYellow, Black` (selected item) | `accent_on_selection()` |
| `Red, Blue` (menu error line) | keep `(primary_fg, primary_bg)` but add a new `error_fg` slot? **Decision:** reuse `accent_fg` on `primary_bg` — inline menu errors pop via accent color, avoiding a 10th slot. Red still signals via full-panel `render_error` flow. Callout for sign-off. |
| `White, Magenta` (holder view) | `fg_on_primary()` (aligned with viewer per your call) |
| `White, Red` (error panel) | `error_panel()` |
| `Blue, Black` (summary border) | `summary_border()` |
| `DarkYellow, Black` (summary stats) | `summary_accent()` |
| `Green, Black` (perfect round) | `summary_success()` |

`flash_screen` keeps hardcoded `Green`/`Red`. Its reset-back-to-default call changes to `fg_on_primary()` so the flash restore matches the active scheme.

`TerminalGuard::new()` initial `SetColors` call also uses `fg_on_primary()`.

---

## 5. Config Persistence

`crates/client/src/config.rs`:

```rust
#[serde(default = "default_color_scheme")]
pub color_scheme: String,
// default_color_scheme() -> "blue"
```

- `AppConfig::load()` → resolve `color_scheme` via `theme::by_id()`, install into the `OnceLock`/`OnceCell`.
- Unknown id (e.g. user hand-edited JSON) → fall back to default silently.
- Changing the scheme in the menu mutates `AppConfig.color_scheme`, re-installs the active scheme, and saves on menu exit like other settings.

---

## 6. Menu UX

- Add a new Settings row: `Color Scheme  <current scheme display name>`.
- Selecting it opens a picker styled like the Word List picker (`render_list_picker`).
- Items show `display_name` (e.g. "House Stark", "Grayscale").
- Confirming selection: update `AppConfig.color_scheme`, call `theme::set_active(id)`, return to Settings. Next render uses the new palette immediately.
- Escape / back = cancel without applying.

`menu.rs` changes:
- New screen variant for the scheme picker (mirrors `MenuScreen::WordListPicker`).
- Settings row added beneath the existing entries (order TBD — propose: just above the Server/Address section).

---

## 7. File Change Summary

| File | Change |
|---|---|
| `crates/client/src/theme.rs` | **new** — `ColorScheme`, `SCHEMES`, active-scheme `OnceLock`, helpers |
| `crates/client/src/lib.rs` (or `main.rs` `mod` decls) | register `pub mod theme;` |
| `crates/client/src/config.rs` | add `color_scheme: String` field + default, resolve-and-install on load, save on change |
| `crates/client/src/render.rs` | replace hardcoded `Color` refs with scheme-slot helpers; holder view uses primary_bg |
| `crates/client/src/menu.rs` | new Settings row + scheme picker screen + handlers |
| `README.md` | document color scheme setting + list schemes |
| `CLAUDE.md` | add theme module to the module table; note truecolor requirement |
| `TODO.md` | check off the color-scheme item under Next Release + Medium |

No protocol or relay changes — schemes are client-local cosmetic state.

---

## 8. Testing (manual, per CLAUDE.md)

1. Fresh run → default is Blue, looks identical to today.
2. Settings → Color Scheme → picker lists all 13 in declared order.
3. Pick Grayscale → menu, question, lobby, error, holder, summary all in grayscale palette.
4. Pick Pastel → light-bg panels render readably (inverted polarity).
5. Pick House Stark → slate/frost palette applies everywhere.
6. Change scheme mid-session → next render reflects it (no relaunch required).
7. Flash correct/pass stays green/red under every scheme.
8. Kill app, re-run → chosen scheme persists via `~/.guess_up_config.json`.
9. Hand-edit config to `"color_scheme": "bogus"` → app loads with Blue, no panic.
10. Networked: host uses Stark, joiner uses Pastel — each sees its own scheme on its own screen (independent client state).

No automated tests — manual verification per the repo's existing test strategy.

---

## 9. Open Callouts for Sign-off

1. **Inline menu errors** (today `Red` on `Blue`) — proposed: map to `accent_fg` on `primary_bg` rather than add a 10th slot. OK, or add a dedicated `error_fg` slot?
2. **Settings row placement** — top of Settings? Between existing rows? Your call.
3. **Holder view alignment** — confirmed: uses `primary_bg`/`primary_fg`. Magenta bg goes away entirely.

---

## 10. Workflow

- Branch: `color-schemes` off `main`.
- `cargo fmt` + `cargo clippy` clean before PR.
- Update `README.md` and `CLAUDE.md` on the branch before opening the PR.
- Check off the TODO item(s) under Next Release + Medium on the same branch (normal feature-branch flow since other files change).
