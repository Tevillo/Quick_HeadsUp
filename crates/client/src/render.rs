use crate::theme;
use crate::types::{EventSender, GameEvent};
use crossterm::cursor::{DisableBlinking, Hide, MoveTo, Show};
use crossterm::style::{
    Color::{self, Green, Red},
    Colors, SetColors,
};
use crossterm::terminal::{self, Clear, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use protocol::Role;
use std::io::{stdout, Write};
use std::time::Duration;

// ─── Scheme color helpers ────────────────────────────────────────────

fn fg_on_primary() -> Colors {
    let s = theme::active();
    Colors::new(s.primary_fg, s.primary_bg)
}

fn accent_on_primary() -> Colors {
    let s = theme::active();
    Colors::new(s.accent_fg, s.primary_bg)
}

fn accent_on_selection() -> Colors {
    let s = theme::active();
    Colors::new(s.accent_fg, s.selection_bg)
}

fn error_panel() -> Colors {
    let s = theme::active();
    Colors::new(s.primary_fg, s.error_bg)
}

fn summary_border_colors() -> Colors {
    let s = theme::active();
    Colors::new(s.summary_border, s.summary_bg)
}

fn summary_accent_colors() -> Colors {
    let s = theme::active();
    Colors::new(s.summary_accent, s.summary_bg)
}

fn summary_success_colors() -> Colors {
    let s = theme::active();
    Colors::new(s.summary_success, s.summary_bg)
}

// ─── Menu rendering ─────────────────────────────────────────────────

pub enum MenuItem<'a> {
    Label(&'a str),
    Action(&'a str),
    Setting {
        label: &'a str,
        value: &'a str,
    },
    TextInput {
        label: &'a str,
        value: &'a str,
        editing: bool,
    },
    Error(&'a str),
}

impl MenuItem<'_> {
    pub fn is_selectable(&self) -> bool {
        !matches!(self, MenuItem::Label(_) | MenuItem::Error(_))
    }
}

pub fn render_menu(title: &str, items: &[MenuItem], selected: usize, term_size: (u16, u16)) {
    let (tw, th) = term_size;

    // Calculate content width from items
    let item_widths: Vec<usize> = items
        .iter()
        .map(|item| match item {
            MenuItem::Label(s) | MenuItem::Action(s) | MenuItem::Error(s) => s.len(),
            MenuItem::Setting { label, value } => label.len() + value.len() + 4,
            MenuItem::TextInput {
                label,
                value,
                editing,
            } => {
                let cursor = if *editing { "_" } else { "" };
                label.len() + value.len() + cursor.len() + 4
            }
        })
        .collect();

    let content_width = title.len().max(*item_widths.iter().max().unwrap_or(&20)) + 6; // "│ " + content + " │" + padding

    let box_line: String = "―".repeat(content_width - 2);
    let total_height = items.len() as u16 + 4; // top border + title + blank + items + bottom border
    let start_row = th.saturating_sub(total_height) / 2;
    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
        MoveTo(col, start_row),
    );
    print!("┌{}┐", box_line);

    // Title
    let _ = queue!(stdout(), MoveTo(col, start_row + 1));
    let _ = queue!(stdout(), SetColors(accent_on_primary()));
    print!("│ {:^width$} │", title, width = content_width - 4);

    // Blank line under title
    let _ = queue!(stdout(), MoveTo(col, start_row + 2));
    let _ = queue!(stdout(), SetColors(fg_on_primary()));
    print!("│ {:width$} │", "", width = content_width - 4);

    // Track which selectable index we're at
    let mut selectable_idx = 0;

    for (i, item) in items.iter().enumerate() {
        let row = start_row + 3 + i as u16;
        let _ = queue!(stdout(), MoveTo(col, row));

        let is_selected = item.is_selectable() && selectable_idx == selected;
        if item.is_selectable() {
            selectable_idx += 1;
        }

        if matches!(item, MenuItem::Error(_)) {
            let _ = queue!(stdout(), SetColors(accent_on_primary()));
        } else if is_selected {
            let _ = queue!(stdout(), SetColors(accent_on_selection()));
        } else {
            let _ = queue!(stdout(), SetColors(fg_on_primary()));
        }

        let text = match item {
            MenuItem::Label(s) | MenuItem::Error(s) => format!("  {}", s),
            MenuItem::Action(s) => {
                if is_selected {
                    format!("> {}", s)
                } else {
                    format!("  {}", s)
                }
            }
            MenuItem::Setting { label, value } => {
                if is_selected {
                    format!("> {}  {}", label, value)
                } else {
                    format!("  {}  {}", label, value)
                }
            }
            MenuItem::TextInput {
                label,
                value,
                editing,
            } => {
                let display_val = if *editing {
                    format!("{}_", value)
                } else {
                    value.to_string()
                };
                if is_selected {
                    format!("> {}  {}", label, display_val)
                } else {
                    format!("  {}  {}", label, display_val)
                }
            }
        };

        print!("│ {:<width$} │", text, width = content_width - 4);

        // Reset colors after highlighted items
        if is_selected || matches!(item, MenuItem::Error(_)) {
            let _ = queue!(stdout(), SetColors(fg_on_primary()));
        }
    }

    // Bottom border
    let bottom_row = start_row + 3 + items.len() as u16;
    let _ = queue!(stdout(), MoveTo(col, bottom_row));
    let _ = queue!(stdout(), SetColors(fg_on_primary()));
    print!("└{}┘", box_line);

    let _ = stdout().flush();
}

pub fn render_list_picker(
    title: &str,
    items: &[String],
    selected: usize,
    scroll_offset: usize,
    term_size: (u16, u16),
) {
    let (tw, th) = term_size;
    let visible_count = 15usize.min(items.len());
    let content_width = items
        .iter()
        .map(|c| c.len())
        .max()
        .unwrap_or(10)
        .max(title.len())
        + 6;

    let box_line: String = "―".repeat(content_width - 2);
    let total_height = visible_count as u16 + 4; // borders + title + blank
    let start_row = th.saturating_sub(total_height) / 2;
    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
        MoveTo(col, start_row),
    );
    print!("┌{}┐", box_line);

    // Title
    let _ = queue!(stdout(), MoveTo(col, start_row + 1));
    let _ = queue!(stdout(), SetColors(accent_on_primary()));
    print!("│ {:^width$} │", title, width = content_width - 4);

    // Scroll indicator top
    let _ = queue!(stdout(), MoveTo(col, start_row + 2));
    let _ = queue!(stdout(), SetColors(fg_on_primary()));
    if scroll_offset > 0 {
        print!("│ {:^width$} │", "▲ more ▲", width = content_width - 4);
    } else {
        print!("│ {:width$} │", "", width = content_width - 4);
    }

    // Visible items
    for i in 0..visible_count {
        let idx = scroll_offset + i;
        let row = start_row + 3 + i as u16;
        let _ = queue!(stdout(), MoveTo(col, row));

        let is_selected = idx == selected;
        if is_selected {
            let _ = queue!(stdout(), SetColors(accent_on_selection()));
        } else {
            let _ = queue!(stdout(), SetColors(fg_on_primary()));
        }

        let prefix = if is_selected { "> " } else { "  " };
        let name = &items[idx];
        print!("│ {}{:<width$} │", prefix, name, width = content_width - 6);

        if is_selected {
            let _ = queue!(stdout(), SetColors(fg_on_primary()));
        }
    }

    // Scroll indicator bottom
    let bottom_indicator_row = start_row + 3 + visible_count as u16;
    let _ = queue!(stdout(), MoveTo(col, bottom_indicator_row));
    let _ = queue!(stdout(), SetColors(fg_on_primary()));
    if scroll_offset + visible_count < items.len() {
        print!("│ {:^width$} │", "▼ more ▼", width = content_width - 4);
    } else {
        print!("│ {:width$} │", "", width = content_width - 4);
    }

    // Bottom border
    let _ = queue!(stdout(), MoveTo(col, bottom_indicator_row + 1));
    print!("└{}┘", box_line);

    let _ = stdout().flush();
}

pub fn render_category_picker(
    categories: &[String],
    selected: usize,
    scroll_offset: usize,
    term_size: (u16, u16),
) {
    render_list_picker(
        "SELECT CATEGORY",
        categories,
        selected,
        scroll_offset,
        term_size,
    );
}

pub fn render_word_list_picker(
    lists: &[String],
    selected: usize,
    scroll_offset: usize,
    term_size: (u16, u16),
) {
    render_list_picker(
        "SELECT WORD LIST",
        lists,
        selected,
        scroll_offset,
        term_size,
    );
}

// ─── Color scheme picker (list + live-preview panel) ────────────────

pub fn render_color_scheme_picker(
    items: &[String],
    selected: usize,
    scroll_offset: usize,
    term_size: (u16, u16),
    preview: &theme::ColorScheme,
) {
    let (tw, th) = term_size;
    let visible_count = 15usize.min(items.len());

    let list_title = "SELECT COLOR SCHEME";
    let list_content_width = items
        .iter()
        .map(|c| c.len())
        .max()
        .unwrap_or(10)
        .max(list_title.len())
        + 6;
    let list_height = (visible_count + 5) as u16;

    let preview_content_width: usize = 36;
    let preview_total_width = preview_content_width as u16 + 2; // side borders
    let preview_rows = preview_sample_rows(preview, preview_content_width);
    let preview_height = preview_rows.len() as u16 + 2; // top + bottom borders

    let gap: u16 = 3;
    let total_width = list_content_width as u16 + gap + preview_total_width;
    let total_height = list_height.max(preview_height);
    let start_row = th.saturating_sub(total_height) / 2;
    let start_col = center_col(tw, total_width);
    let list_col = start_col;
    let preview_col = start_col + list_content_width as u16 + gap;

    // Clear once in the active (already-committed) scheme.
    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
    );

    // --- List box ---
    let box_line: String = "―".repeat(list_content_width - 2);
    let _ = queue!(
        stdout(),
        MoveTo(list_col, start_row),
        SetColors(fg_on_primary()),
    );
    print!("┌{}┐", box_line);

    let _ = queue!(
        stdout(),
        MoveTo(list_col, start_row + 1),
        SetColors(accent_on_primary()),
    );
    print!("│ {:^width$} │", list_title, width = list_content_width - 4);

    let _ = queue!(
        stdout(),
        MoveTo(list_col, start_row + 2),
        SetColors(fg_on_primary()),
    );
    if scroll_offset > 0 {
        print!("│ {:^width$} │", "▲ more ▲", width = list_content_width - 4);
    } else {
        print!("│ {:width$} │", "", width = list_content_width - 4);
    }

    for i in 0..visible_count {
        let idx = scroll_offset + i;
        let row = start_row + 3 + i as u16;
        let _ = queue!(stdout(), MoveTo(list_col, row));
        let is_selected = idx == selected;
        if is_selected {
            let _ = queue!(stdout(), SetColors(accent_on_selection()));
        } else {
            let _ = queue!(stdout(), SetColors(fg_on_primary()));
        }
        let prefix = if is_selected { "> " } else { "  " };
        let name = &items[idx];
        print!(
            "│ {}{:<width$} │",
            prefix,
            name,
            width = list_content_width - 6
        );
    }

    let bottom_indicator_row = start_row + 3 + visible_count as u16;
    let _ = queue!(
        stdout(),
        MoveTo(list_col, bottom_indicator_row),
        SetColors(fg_on_primary()),
    );
    if scroll_offset + visible_count < items.len() {
        print!("│ {:^width$} │", "▼ more ▼", width = list_content_width - 4);
    } else {
        print!("│ {:width$} │", "", width = list_content_width - 4);
    }

    let _ = queue!(stdout(), MoveTo(list_col, bottom_indicator_row + 1));
    print!("└{}┘", box_line);

    // --- Preview panel (uses hovered scheme's colors, wrapped in a border
    //     drawn in the active scheme to mark it off from the surrounding UI) ---
    let preview_border_line: String = "─".repeat(preview_content_width);
    let right_border_col = preview_col + 1 + preview_content_width as u16;

    let _ = queue!(
        stdout(),
        MoveTo(preview_col, start_row),
        SetColors(fg_on_primary()),
    );
    print!("┌{}┐", preview_border_line);

    for (i, (text, colors)) in preview_rows.iter().enumerate() {
        let row = start_row + 1 + i as u16;

        let _ = queue!(
            stdout(),
            MoveTo(preview_col, row),
            SetColors(fg_on_primary()),
        );
        print!("│");

        let _ = queue!(stdout(), MoveTo(preview_col + 1, row), SetColors(*colors));
        print!("{}", text);

        let _ = queue!(
            stdout(),
            MoveTo(right_border_col, row),
            SetColors(fg_on_primary()),
        );
        print!("│");
    }

    let preview_bottom_row = start_row + 1 + preview_rows.len() as u16;
    let _ = queue!(
        stdout(),
        MoveTo(preview_col, preview_bottom_row),
        SetColors(fg_on_primary()),
    );
    print!("└{}┘", preview_border_line);

    // Reset for following text output (if any) to the active scheme.
    let _ = queue!(stdout(), SetColors(fg_on_primary()));
    let _ = stdout().flush();
}

fn preview_sample_rows(scheme: &theme::ColorScheme, width: usize) -> Vec<(String, Colors)> {
    let pri = Colors::new(scheme.primary_fg, scheme.primary_bg);
    let title = Colors::new(scheme.accent_fg, scheme.primary_bg);
    let sel = Colors::new(scheme.accent_fg, scheme.selection_bg);
    let sum_border = Colors::new(scheme.summary_border, scheme.summary_bg);
    let sum_accent = Colors::new(scheme.summary_accent, scheme.summary_bg);
    let sum_success = Colors::new(scheme.summary_success, scheme.summary_bg);
    let err = Colors::new(scheme.primary_fg, scheme.error_bg);
    let pad = |s: &str| -> String { format!("{:<width$}", s, width = width) };
    let heading = format!("  PREVIEW — {}", scheme.name);
    vec![
        (pad(&heading), title),
        (pad(""), pri),
        (pad("  Menu text"), pri),
        (pad("  > Selected item"), sel),
        (pad("  Title accent"), title),
        (pad(""), pri),
        (pad("  ═══ Summary ═══"), sum_border),
        (pad("  Score: 42 / 50"), sum_accent),
        (pad("  Perfect round!"), sum_success),
        (pad(""), pri),
        (pad("  ERROR sample"), err),
        (pad(""), pri),
    ]
}

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> Self {
        let _ = terminal::enable_raw_mode();
        let _ = execute!(
            stdout(),
            EnterAlternateScreen,
            terminal::SetTitle("ASOIAF Guess Up"),
            SetColors(fg_on_primary()),
            DisableBlinking,
            Hide,
            Clear(terminal::ClearType::All),
        );
        TerminalGuard
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Immediately exit the process, restoring terminal state first.
pub fn force_exit() -> ! {
    let _ = execute!(stdout(), Show, LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
    std::process::exit(0);
}

pub fn terminal_size() -> (u16, u16) {
    terminal::size().unwrap_or((80, 24))
}

fn center_col(terminal_width: u16, content_width: u16) -> u16 {
    terminal_width.saturating_sub(content_width) / 2
}

// ─── Low-time warning ────────────────────────────────────────────────

/// Visual state for the low-time warning. When `timer_red` is true the timer
/// text renders in red; when `border_red` is true a red border is drawn along
/// the outer edge of the terminal. `border_red` toggles every 500ms for the
/// 2Hz blink, while `timer_red` stays on continuously during the warning.
#[derive(Debug, Clone, Copy, Default)]
pub struct WarningState {
    pub timer_red: bool,
    pub border_red: bool,
}

impl WarningState {
    pub const OFF: WarningState = WarningState {
        timer_red: false,
        border_red: false,
    };

    /// Build the state from the remaining seconds and the current blink phase.
    /// When `seconds_left > WARNING_THRESHOLD_SECS` the warning is fully off.
    pub fn from(seconds_left: u64, blink_on: bool) -> Self {
        if seconds_left <= crate::timer::WARNING_THRESHOLD_SECS {
            WarningState {
                timer_red: true,
                border_red: blink_on,
            }
        } else {
            WarningState::OFF
        }
    }
}

/// Render a centered timer cell; if `red` is true, color the text red on the
/// active primary background. Restores the active-scheme colors afterward.
fn render_timer_cell(text: &str, red: bool) {
    if red {
        let bg = theme::active().primary_bg;
        let _ = queue!(stdout(), SetColors(Colors::new(Red, bg)));
        print!("{}", text);
        let _ = queue!(stdout(), SetColors(fg_on_primary()));
    } else {
        print!("{}", text);
    }
}

/// Draw a red border along the outer edges of the terminal as an overlay. The
/// inner content has already been drawn; this only touches the four border
/// rows/columns. No-op unless `state.border_red` is true.
fn draw_warning_border(state: WarningState, term_size: (u16, u16)) {
    if !state.border_red {
        return;
    }
    let (tw, th) = term_size;
    if tw < 2 || th < 2 {
        return;
    }
    let bg = theme::active().primary_bg;
    let _ = queue!(stdout(), SetColors(Colors::new(Red, bg)));

    let top: String = "═".repeat((tw - 2) as usize);
    let bottom = top.clone();

    let _ = queue!(stdout(), MoveTo(0, 0));
    print!("╔{}╗", top);

    for row in 1..th - 1 {
        let _ = queue!(stdout(), MoveTo(0, row));
        print!("║");
        let _ = queue!(stdout(), MoveTo(tw - 1, row));
        print!("║");
    }

    let _ = queue!(stdout(), MoveTo(0, th - 1));
    print!("╚{}╝", bottom);

    let _ = queue!(stdout(), SetColors(fg_on_primary()));
}

pub fn render_question(
    word: &str,
    seconds_left: u64,
    score: usize,
    warning: WarningState,
    term_size: (u16, u16),
) {
    let (tw, th) = term_size;
    let mid_row = th / 2;

    let timer_line = format!("{:02} Seconds Left  |  Score: {}", seconds_left, score);
    let content_width = word.len().max(timer_line.len()) + 4; // "│ " + content + " │"
    let box_top: String = "―".repeat(content_width - 2);
    let word_padded = format!("{:^width$}", word, width = content_width - 4);
    let timer_padded = format!("{:^width$}", timer_line, width = content_width - 4);
    let box_bottom: String = "―".repeat(content_width - 2);

    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
        MoveTo(col, mid_row),
    );
    print!("┌{}┐", box_top);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 1));
    print!("│ {} │", word_padded);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 2));
    print!("│ ");
    render_timer_cell(&timer_padded, warning.timer_red);
    print!(" │");
    let _ = queue!(stdout(), MoveTo(col, mid_row + 3));
    print!("└{}┘", box_bottom);
    draw_warning_border(warning, term_size);
    let _ = stdout().flush();
}

pub fn render_question_unlimited(word: &str, score: usize, term_size: (u16, u16)) {
    let (tw, th) = term_size;
    let mid_row = th / 2;

    let status_line = "LAST QUESTION — No Time Limit";
    let score_line = format!("Score: {}", score);
    let content_width = word.len().max(status_line.len()).max(score_line.len()) + 4;
    let box_top: String = "―".repeat(content_width - 2);
    let word_padded = format!("{:^width$}", word, width = content_width - 4);
    let status_padded = format!("{:^width$}", status_line, width = content_width - 4);
    let score_padded = format!("{:^width$}", score_line, width = content_width - 4);
    let box_bottom: String = "―".repeat(content_width - 2);

    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
        MoveTo(col, mid_row),
    );
    print!("┌{}┐", box_top);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 1));
    print!("│ {} │", word_padded);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 2));
    print!("│ {} │", status_padded);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 3));
    print!("│ {} │", score_padded);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 4));
    print!("└{}┘", box_bottom);
    let _ = stdout().flush();
}

pub async fn flash_screen(color: Color, tx: EventSender) {
    let _ = execute!(
        stdout(),
        SetColors(Colors::new(Color::Black, color)),
        Clear(terminal::ClearType::All),
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    let _ = execute!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
    );
    let _ = tx.send(GameEvent::Redraw).await;
}

pub fn flash_correct(tx: EventSender) {
    tokio::spawn(flash_screen(Green, tx));
}

pub fn flash_incorrect(tx: EventSender) {
    tokio::spawn(flash_screen(Red, tx));
}

pub fn bell() {
    print!("\x07");
    let _ = stdout().flush();
}

fn big_digit(n: u8) -> &'static [&'static str] {
    match n {
        3 => &[
            "██████╗ ",
            "╚════██╗",
            " █████╔╝",
            " ╚═══██╗",
            "██████╔╝",
            "╚═════╝ ",
        ],
        2 => &[
            "██████╗ ",
            "╚════██╗",
            " █████╔╝",
            "██╔═══╝ ",
            "███████╗",
            "╚══════╝",
        ],
        1 => &[" ██╗", "███║", "╚██║", " ██║", " ██║", " ╚═╝"],
        _ => &[],
    }
}

pub fn render_countdown(term_size: (u16, u16)) {
    let (tw, th) = term_size;
    let mid_row = th.saturating_sub(6) / 2;

    for i in (1u8..=3).rev() {
        let lines = big_digit(i);
        // Width of the widest line in the digit
        let digit_width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);

        let col = center_col(tw, digit_width as u16);

        let _ = queue!(
            stdout(),
            SetColors(fg_on_primary()),
            Clear(terminal::ClearType::All),
        );
        for (row, line) in lines.iter().enumerate() {
            let _ = queue!(stdout(), MoveTo(col, mid_row + row as u16));
            print!("{}", line);
        }
        let _ = stdout().flush();
        std::thread::sleep(Duration::from_secs(1));
    }
    let _ = execute!(stdout(), Clear(terminal::ClearType::All));
}

/// Render the end-of-game summary + a list of action hints inside the alt screen.
///
/// Callers own the `TerminalGuard`; this function draws a single centered box and
/// does not clear it afterward. It's used by solo, host, and joiner flows —
/// `actions` are the trailing lines below the stats (e.g. `"[P] Play again"`,
/// `"Press any key to continue..."`, or `"Waiting for host..."`).
///
/// `session_tally` is `Some((score, total))` when called from the host in
/// auto-rotate Holder mode — it adds a "Session: X / Y" row above the missed
/// words so the running session total is visible across rounds.
#[allow(clippy::too_many_arguments)]
pub fn render_game_summary(
    score: usize,
    total_questions: usize,
    missed_words: &[String],
    game_time: u64,
    all_used: bool,
    session_tally: Option<(usize, usize)>,
    actions: &[&str],
    term_size: (u16, u16),
) {
    let (tw, th) = term_size;
    let inner: usize = 48; // text area width inside the box

    let passed = total_questions.saturating_sub(score);
    let accuracy = if total_questions > 0 {
        (score as f64 / total_questions as f64) * 100.0
    } else {
        0.0
    };
    let pace = if game_time > 0 {
        total_questions as f64 / game_time as f64 * 60.0
    } else {
        0.0
    };

    #[derive(Clone, Copy)]
    enum RowKind {
        Accent,
        Success,
        Divider,
    }
    let mut rows: Vec<(String, RowKind)> = Vec::new();

    let center_row = |text: &str| format!("{:^w$}", text, w = inner);

    rows.push((center_row("GAME OVER"), RowKind::Accent));
    rows.push((
        center_row(&format!("Score: {} / {}", score, total_questions)),
        RowKind::Accent,
    ));
    rows.push((
        center_row(&format!("Correct: {}  |  Passed: {}", score, passed)),
        RowKind::Accent,
    ));
    rows.push((
        center_row(&format!("Accuracy: {:.0}%", accuracy)),
        RowKind::Accent,
    ));
    rows.push((
        center_row(&format!("Pace: {:.1} answers/min", pace)),
        RowKind::Accent,
    ));
    if all_used {
        rows.push((center_row("You cleared the entire list!"), RowKind::Success));
    }

    if let Some((sess_score, sess_total)) = session_tally {
        rows.push((String::new(), RowKind::Divider));
        rows.push((
            center_row(&format!("Session: {} / {}", sess_score, sess_total)),
            RowKind::Success,
        ));
    }

    rows.push((String::new(), RowKind::Divider));

    if missed_words.is_empty() {
        rows.push((
            center_row("No missed words — perfect round!"),
            RowKind::Success,
        ));
    } else {
        rows.push((center_row("Missed words:"), RowKind::Accent));
        let missed_inner = inner.saturating_sub(4);
        let missed_lines = build_missed_lines(missed_words, missed_inner, 3);
        for line in missed_lines {
            rows.push((
                format!("  {:<w$}  ", line, w = missed_inner),
                RowKind::Accent,
            ));
        }
    }

    if !actions.is_empty() {
        rows.push((String::new(), RowKind::Divider));
        for action in actions {
            rows.push((format!("  {:<w$}", action, w = inner - 2), RowKind::Accent));
        }
    }

    // --- Render ---
    let box_line: String = "─".repeat(inner + 2);
    let total_height = rows.len() as u16 + 2; // top + bottom border
    let start_row = th.saturating_sub(total_height) / 2;
    let col = center_col(tw, (inner + 4) as u16);

    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
        MoveTo(col, start_row),
        SetColors(summary_border_colors()),
    );
    print!("┌{}┐", box_line);

    for (i, (text, kind)) in rows.iter().enumerate() {
        let row = start_row + 1 + i as u16;
        let _ = queue!(stdout(), MoveTo(col, row));
        match kind {
            RowKind::Accent => {
                let _ = queue!(stdout(), SetColors(summary_accent_colors()));
                print!("│ {} │", text);
            }
            RowKind::Success => {
                let _ = queue!(stdout(), SetColors(summary_success_colors()));
                print!("│ {} │", text);
            }
            RowKind::Divider => {
                let _ = queue!(stdout(), SetColors(summary_border_colors()));
                print!("├{}┤", box_line);
            }
        }
    }

    let bottom_row = start_row + 1 + rows.len() as u16;
    let _ = queue!(
        stdout(),
        MoveTo(col, bottom_row),
        SetColors(summary_border_colors()),
    );
    print!("└{}┘", box_line);
    let _ = queue!(stdout(), SetColors(fg_on_primary()));
    let _ = stdout().flush();
}

/// Wrap a list of missed words into lines no wider than `width`, capping the
/// total line count at `max_lines`. When the list is too long, the final line
/// becomes `"...and N more"`.
fn build_missed_lines(missed: &[String], width: usize, max_lines: usize) -> Vec<String> {
    debug_assert!(max_lines >= 1);

    let mut lines: Vec<String> = Vec::new();
    let mut line_counts: Vec<usize> = Vec::new();
    let mut current = String::new();
    let mut current_count = 0usize;

    for word in missed {
        let sep_len = if current.is_empty() { 0 } else { 2 };
        let fits = current.len() + sep_len + word.len() <= width;
        if !fits && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            line_counts.push(current_count);
            current_count = 0;
        }
        if !current.is_empty() {
            current.push_str(", ");
        }
        current.push_str(word);
        current_count += 1;
    }
    if !current.is_empty() {
        lines.push(current);
        line_counts.push(current_count);
    }

    if lines.len() <= max_lines {
        return lines;
    }

    let keep = max_lines.saturating_sub(1);
    let kept_count: usize = line_counts[..keep].iter().sum();
    let remaining = missed.len().saturating_sub(kept_count);
    let mut truncated: Vec<String> = lines.into_iter().take(keep).collect();
    truncated.push(format!("...and {} more", remaining));
    truncated
}

// ─── Holder view (networked: shows timer + score but NOT the word) ───

pub fn render_holder_view(
    seconds_left: u64,
    score: usize,
    warning: WarningState,
    term_size: (u16, u16),
) {
    let (tw, th) = term_size;
    let mid_row = th / 2;

    let placeholder = "GUESS!";
    let timer_line = format!("{:02} Seconds Left  |  Score: {}", seconds_left, score);
    let hint_line = "Press [Y] Correct  [N] Pass";
    let content_width = timer_line.len().max(hint_line.len()).max(placeholder.len()) + 4;
    let box_line: String = "―".repeat(content_width - 2);
    let timer_padded = format!("{:^width$}", timer_line, width = content_width - 4);

    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(fg_on_primary()),
        Clear(terminal::ClearType::All),
        MoveTo(col, mid_row),
    );
    print!("┌{}┐", box_line);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 1));
    print!("│ {:^width$} │", placeholder, width = content_width - 4);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 2));
    print!("│ ");
    render_timer_cell(&timer_padded, warning.timer_red);
    print!(" │");
    let _ = queue!(stdout(), MoveTo(col, mid_row + 3));
    print!("│ {:^width$} │", hint_line, width = content_width - 4);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 4));
    print!("└{}┘", box_line);
    draw_warning_border(warning, term_size);
    let _ = stdout().flush();
}

// ─── Lobby rendering ─────────────────────────────────────────────────

fn render_centered_box(lines: &[&str], term_size: (u16, u16), colors: Colors) {
    let (tw, th) = term_size;
    let content_width = lines.iter().map(|l| l.len()).max().unwrap_or(20) + 4;
    let box_line: String = "―".repeat(content_width - 2);
    let total_height = lines.len() as u16 + 2; // top + bottom border
    let start_row = th.saturating_sub(total_height) / 2;
    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(colors),
        Clear(terminal::ClearType::All),
        MoveTo(col, start_row),
    );
    print!("┌{}┐", box_line);
    for (i, line) in lines.iter().enumerate() {
        let _ = queue!(stdout(), MoveTo(col, start_row + 1 + i as u16));
        print!("│ {:^width$} │", line, width = content_width - 4);
    }
    let _ = queue!(stdout(), MoveTo(col, start_row + 1 + lines.len() as u16));
    print!("└{}┘", box_line);
    let _ = stdout().flush();
}

pub fn render_joined_room(room_code: &str, term_size: (u16, u16)) {
    let code_line = format!("Joined room: {}", room_code);
    let lines = [
        "HEADS UP — JOINED",
        "",
        &code_line,
        "",
        "Waiting for host...",
    ];
    render_centered_box(&lines, term_size, fg_on_primary());
}

pub fn render_role_assigned(role: Role, term_size: (u16, u16)) {
    let role_line = format!("You are the: {}", role);
    let desc = match role {
        Role::Viewer => "You'll see the words and give verbal clues",
        Role::Holder => "You'll guess and press [Y] Correct / [N] Pass",
    };
    let lines = [
        "ROLE ASSIGNED",
        "",
        &role_line,
        desc,
        "",
        "Game starting...",
    ];
    render_centered_box(&lines, term_size, fg_on_primary());
}

pub fn render_message(msg: &str, term_size: (u16, u16)) {
    let lines = [msg];
    render_centered_box(&lines, term_size, fg_on_primary());
}

pub fn render_error(msg: &str, term_size: (u16, u16)) {
    let lines = ["ERROR", "", msg, "", "Press any key to continue..."];
    render_centered_box(&lines, term_size, error_panel());
}

/// Result screen for the word-list import flow. Handles both success and
/// failure — callers pass `success=true` for the imported-OK message and
/// `success=false` for any error (parse failure, filesystem error, or
/// empty imports dir).
pub fn render_import_result(msg: &str, success: bool, term_size: (u16, u16)) {
    let title = if success {
        "IMPORT COMPLETE"
    } else {
        "IMPORT ERROR"
    };
    let colors = if success {
        fg_on_primary()
    } else {
        error_panel()
    };
    // Split msg on newlines so a multi-line message (e.g. "prefix\npath")
    // renders each line on its own row inside the box.
    let mut lines: Vec<&str> = vec![title, ""];
    lines.extend(msg.split('\n'));
    lines.push("");
    lines.push("Press any key to return...");
    render_centered_box(&lines, term_size, colors);
}
