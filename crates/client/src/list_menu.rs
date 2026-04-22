//! Shared helpers for list-style TUI screens.
//!
//! All of the "pick an index from a list" screens (main menu, category
//! picker, word list picker, holder picker, post-game menu) used to carry
//! identical blocks of up/down/enter/cancel/Ctrl-C handling and scroll
//! bookkeeping. This module centralises that logic as pure, synchronous
//! helpers — each screen keeps its own render call and its own Enter/Cancel
//! dispatch, but routes key events through `classify_key` and tracks its
//! cursor in a `ListState`.
//!
//! Text-input screens (server connect, join room) are out of scope — they
//! mix character input with list navigation and are better served by their
//! bespoke handlers.

use crossterm::event::{KeyCode, KeyEvent};

/// Cursor + scroll offset for a list-style screen. `selected` is the
/// absolute index into the underlying items slice; `scroll_offset` is the
/// index of the first on-screen row. Screens without scrolling can ignore
/// `scroll_offset`.
pub struct ListState {
    pub selected: usize,
    pub scroll_offset: usize,
}

impl ListState {
    pub fn new(initial: usize) -> Self {
        Self {
            selected: initial,
            scroll_offset: 0,
        }
    }

    /// Move the cursor up, wrapping to the last item when already at the top.
    /// `len` is the number of items currently in the list.
    pub fn on_up(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = self.selected.checked_sub(1).unwrap_or(len - 1);
    }

    /// Move the cursor down, wrapping to the first item when at the bottom.
    pub fn on_down(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    /// Adjust `scroll_offset` so that `selected` is within the visible
    /// window. Safe to call before every render; a no-op when already in
    /// range.
    pub fn ensure_visible(&mut self, visible: usize, _len: usize) {
        if visible == 0 {
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + visible {
            self.scroll_offset = self.selected + 1 - visible;
        }
    }
}

/// Semantic classification of a key event for list-style screens.
/// `Unhandled` means the caller may apply its own bindings (e.g. the
/// settings screen's h/l value cycling) before falling through.
pub enum ListKey {
    Up,
    Down,
    Enter,
    Cancel,
    Unhandled,
}

/// Classify a KeyEvent. `Up/k` → Up, `Down/j` → Down, `Enter` → Enter,
/// `Esc/q` → Cancel, anything else → Unhandled.
///
/// Ctrl-C is intentionally *not* handled here — the caller must check
/// `input::is_ctrl_c` and invoke `render::force_exit` before classifying,
/// so force-exit behaviour stays consistent with input-task paths.
pub fn classify_key(key: &KeyEvent) -> ListKey {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => ListKey::Up,
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => ListKey::Down,
        KeyCode::Enter => ListKey::Enter,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => ListKey::Cancel,
        _ => ListKey::Unhandled,
    }
}
