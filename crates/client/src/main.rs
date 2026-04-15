mod config;
mod game;
mod input;
mod lobby;
mod menu;
mod net;
mod render;
mod timer;
mod types;

use config::AppConfig;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use types::*;

pub fn load_words(path: &str, category: Option<&str>) -> Vec<String> {
    let content = fs::read_to_string(Path::new(path)).expect("Could not read word file");

    let mut words = Vec::new();
    let mut current_category: Option<String> = None;
    let mut seen = HashSet::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Check for category header like [House Stark]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_category = Some(trimmed[1..trimmed.len() - 1].to_string());
            continue;
        }

        // Filter by category if specified
        if let Some(cat) = category {
            match &current_category {
                Some(cur) if cur.eq_ignore_ascii_case(cat) => {}
                _ => continue,
            }
        }

        if seen.insert(trimmed.to_lowercase()) {
            words.push(trimmed.to_string());
        }
    }

    words
}

pub fn load_categories(path: &str) -> Vec<String> {
    let Ok(content) = fs::read_to_string(Path::new(path)) else {
        return Vec::new();
    };
    let mut categories = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            categories.push(trimmed[1..trimmed.len() - 1].to_string());
        }
    }
    categories
}

#[tokio::main]
async fn main() {
    let mut config = AppConfig::load();

    loop {
        let _guard = render::TerminalGuard::new();
        let action = menu::menu_loop(&mut config).await;
        drop(_guard);

        config.save();

        match action {
            menu::MenuAction::Solo => run_solo(&config).await,
            menu::MenuAction::Host { relay_addr } => run_host(&mut config, &relay_addr).await,
            menu::MenuAction::Join {
                relay_addr,
                room_code,
            } => run_join(&config, &relay_addr, &room_code).await,
            menu::MenuAction::Quit => break,
        }
    }
}

async fn run_solo(app_config: &AppConfig) {
    let words = load_words(&app_config.word_file, app_config.category.as_deref());
    if words.is_empty() {
        show_error(&format!(
            "No words found in '{}' (category: {:?})",
            app_config.word_file, app_config.category
        ))
        .await;
        return;
    }

    let config = app_config.to_game_config();

    // Set up terminal (guard ensures cleanup on drop/panic)
    let _guard = render::TerminalGuard::new();

    // Countdown animation
    if !config.skip_countdown {
        let term_size = render::terminal_size();
        render::render_countdown(term_size);
    }

    // Set up channels
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<GameEvent>(32);
    let (bonus_tx, bonus_rx) = tokio::sync::mpsc::channel::<u64>(16);

    let input_tx = event_tx.clone();
    let timer_tx = event_tx.clone();
    let flash_tx = event_tx.clone();

    let bonus_sender = match &config.mode {
        GameMode::ExtraTime { .. } => Some(bonus_tx),
        GameMode::Normal => {
            drop(bonus_tx);
            None
        }
    };

    // Spawn input and timer tasks
    let input_handle = tokio::spawn(input::input_task(input_tx));
    let timer_handle = tokio::spawn(timer::timer_task(timer_tx, app_config.game_time, bonus_rx));

    // Run game loop (blocks until game ends)
    let summary = game::run_game(config, words, event_rx, bonus_sender, flash_tx, None, None).await;

    // Abort background tasks
    input_handle.abort();
    timer_handle.abort();

    // Restore terminal before printing summary
    drop(_guard);

    render::print_output(
        summary.score,
        summary.total_questions,
        &summary.missed_words,
        summary.game_time,
        summary.all_used,
    );
}

async fn run_host(app_config: &mut AppConfig, relay_addr: &str) {
    if let Err(e) = lobby::run_host_session(relay_addr, app_config).await {
        show_error(&format!("{}", e)).await;
    }
}

async fn run_join(app_config: &AppConfig, relay_addr: &str, room_code: &str) {
    if let Err(e) = lobby::run_joiner_session(relay_addr, room_code, app_config).await {
        show_error(&format!("{}", e)).await;
    }
}

async fn show_error(msg: &str) {
    let _guard = render::TerminalGuard::new();
    let term_size = render::terminal_size();
    render::render_error(msg, term_size);

    // Wait for any keypress
    let mut reader = EventStream::new();
    while let Some(Ok(event)) = reader.next().await {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                break;
            }
        }
    }
}
