use crate::types::{EventSender, GameEvent};
use crossterm::cursor::{DisableBlinking, Hide, MoveTo, Show};
use crossterm::style::{
    Color::{self, Black, Blue, DarkYellow, Green, Magenta, Red, White},
    Colors, SetColors,
};
use crossterm::terminal::{self, Clear, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use protocol::Role;
use std::io::{stdout, Write};
use std::time::Duration;

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> Self {
        let _ = terminal::enable_raw_mode();
        let _ = execute!(
            stdout(),
            EnterAlternateScreen,
            terminal::SetTitle("ASOIAF Heads Up"),
            SetColors(Colors::new(White, Blue)),
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

pub fn terminal_size() -> (u16, u16) {
    terminal::size().unwrap_or((80, 24))
}

fn center_col(terminal_width: u16, content_width: u16) -> u16 {
    terminal_width.saturating_sub(content_width) / 2
}

pub fn render_question(word: &str, seconds_left: u64, score: usize, term_size: (u16, u16)) {
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
        SetColors(Colors::new(White, Blue)),
        Clear(terminal::ClearType::All),
        MoveTo(col, mid_row),
    );
    print!("┌{}┐", box_top);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 1));
    print!("│ {} │", word_padded);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 2));
    print!("│ {} │", timer_padded);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 3));
    print!("└{}┘", box_bottom);
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
        SetColors(Colors::new(White, Blue)),
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
        SetColors(Colors::new(Black, color)),
        Clear(terminal::ClearType::All),
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    let _ = execute!(
        stdout(),
        SetColors(Colors::new(White, Blue)),
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
            SetColors(Colors::new(White, Blue)),
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

pub fn print_output(
    score: usize,
    total_questions: usize,
    missed_words: &[String],
    game_time: u64,
    all_used: bool,
) {
    let divider = "═".repeat(50);
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

    let _ = execute!(stdout(), SetColors(Colors::new(Blue, Black)));
    println!("\n  ╔{}╗", divider);
    println!("  ║{:^50}║", "GAME OVER");
    println!("  ╠{}╣", divider);

    let _ = execute!(stdout(), SetColors(Colors::new(DarkYellow, Black)));
    println!(
        "  ║{:^50}║",
        format!("Score: {} / {}", score, total_questions)
    );
    println!(
        "  ║{:^50}║",
        format!("Correct: {}  |  Passed: {}", score, passed)
    );
    println!("  ║{:^50}║", format!("Accuracy: {:.0}%", accuracy));
    println!("  ║{:^50}║", format!("Pace: {:.1} answers/min", pace));

    if all_used {
        let _ = execute!(stdout(), SetColors(Colors::new(Green, Black)));
        println!("  ║{:^50}║", "You cleared the entire list!");
    }

    let _ = execute!(stdout(), SetColors(Colors::new(Blue, Black)));
    println!("  ╠{}╣", divider);

    if missed_words.is_empty() {
        let _ = execute!(stdout(), SetColors(Colors::new(DarkYellow, Black)));
        println!("  ║{:^50}║", "No missed words — perfect round!");
    } else {
        println!("  ║{:^50}║", "Missed words:");
        let _ = execute!(stdout(), SetColors(Colors::new(DarkYellow, Black)));
        // Print missed words, wrapping lines to fit inside the box
        let mut line = String::new();
        for (i, word) in missed_words.iter().enumerate() {
            let separator = if i > 0 { ", " } else { "" };
            if line.len() + separator.len() + word.len() > 46 {
                println!("  ║  {:<48}║", line);
                line = word.clone();
            } else {
                line.push_str(separator);
                line.push_str(word);
            }
        }
        if !line.is_empty() {
            println!("  ║  {:<48}║", line);
        }
    }

    let _ = execute!(stdout(), SetColors(Colors::new(Blue, Black)));
    println!("  ╚{}╝\n", divider);
}

// ─── Holder view (networked: shows timer + score but NOT the word) ───

pub fn render_holder_view(seconds_left: u64, score: usize, term_size: (u16, u16)) {
    let (tw, th) = term_size;
    let mid_row = th / 2;

    let placeholder = "GUESS!";
    let timer_line = format!("{:02} Seconds Left  |  Score: {}", seconds_left, score);
    let hint_line = "Press [Y] Correct  [N] Pass";
    let content_width = timer_line.len().max(hint_line.len()).max(placeholder.len()) + 4;
    let box_line: String = "―".repeat(content_width - 2);

    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(Colors::new(White, Magenta)),
        Clear(terminal::ClearType::All),
        MoveTo(col, mid_row),
    );
    print!("┌{}┐", box_line);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 1));
    print!("│ {:^width$} │", placeholder, width = content_width - 4);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 2));
    print!("│ {:^width$} │", timer_line, width = content_width - 4);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 3));
    print!("│ {:^width$} │", hint_line, width = content_width - 4);
    let _ = queue!(stdout(), MoveTo(col, mid_row + 4));
    print!("└{}┘", box_line);
    let _ = stdout().flush();
}

// ─── Lobby rendering ─────────────────────────────────────────────────

fn render_centered_box(lines: &[&str], term_size: (u16, u16), bg: Color) {
    let (tw, th) = term_size;
    let content_width = lines.iter().map(|l| l.len()).max().unwrap_or(20) + 4;
    let box_line: String = "―".repeat(content_width - 2);
    let total_height = lines.len() as u16 + 2; // top + bottom border
    let start_row = th.saturating_sub(total_height) / 2;
    let col = center_col(tw, content_width as u16);

    let _ = queue!(
        stdout(),
        SetColors(Colors::new(White, bg)),
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

pub fn render_waiting_for_peer(room_code: &str, term_size: (u16, u16)) {
    let code_line = format!("Room: {}", room_code);
    let lines = [
        "HEADS UP — HOST",
        "",
        &code_line,
        "",
        "Waiting for opponent...",
    ];
    render_centered_box(&lines, term_size, Blue);
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
    render_centered_box(&lines, term_size, Blue);
}

pub fn render_role_select(term_size: (u16, u16)) {
    let lines = [
        "CHOOSE YOUR ROLE",
        "",
        "[V] Viewer — See words, give clues",
        "[H] Holder — Guess and press Y/N",
    ];
    render_centered_box(&lines, term_size, Blue);
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
    render_centered_box(&lines, term_size, Blue);
}

pub fn render_post_game_menu(term_size: (u16, u16)) {
    let lines = [
        "WHAT NEXT?",
        "",
        "[P] Play again",
        "[S] Swap roles",
        "[Q] Quit session",
    ];
    render_centered_box(&lines, term_size, Blue);
}

pub fn render_message(msg: &str, term_size: (u16, u16)) {
    let lines = [msg];
    render_centered_box(&lines, term_size, Blue);
}
