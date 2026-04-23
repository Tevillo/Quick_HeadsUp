mod config;
mod converter;
mod converter_menu;
mod game;
mod input;
mod list_menu;
mod lobby;
mod menu;
mod net;
mod paths;
mod render;
mod terminal_spawn;
mod theme;
mod timer;
mod types;

use config::AppConfig;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use net::NetConnection;
use std::collections::HashSet;
use std::fs;
use types::*;

pub fn load_words(filename: &str, category: Option<&str>) -> Vec<String> {
    let Ok(path) = paths::word_file_path(filename) else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return Vec::new();
    };

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

pub fn load_categories(filename: &str) -> Vec<String> {
    let Ok(path) = paths::word_file_path(filename) else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(&path) else {
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
    let skip_spawn = std::env::args().any(|a| a == "--no-spawn-terminal");
    match terminal_spawn::spawn_if_needed(skip_spawn) {
        terminal_spawn::SpawnOutcome::ShouldContinue => {}
        terminal_spawn::SpawnOutcome::Spawned => return,
        terminal_spawn::SpawnOutcome::Failed => std::process::exit(1),
    }

    let mut config = AppConfig::load();
    theme::set_active(&config.color_scheme);

    match paths::list_available_lists() {
        Err(_) => {
            let path_display = paths::lists_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<install_dir>/lists".to_string());
            show_error(&format!(
                "No `lists/` directory found at {}. Create it and add at least one `.txt` word list.",
                path_display
            ))
            .await;
            return;
        }
        Ok(names) if names.is_empty() => {
            let path_display = paths::lists_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<install_dir>/lists".to_string());
            show_error(&format!(
                "No word lists found in {}. Add at least one `.txt` file.",
                path_display
            ))
            .await;
            return;
        }
        Ok(names) => {
            if !names.iter().any(|n| n == &config.word_file) {
                config.word_file = names[0].clone();
                config.category = None;
                config.save();
            }
        }
    }

    menu::menu_loop(&mut config).await;
    config.save();
}

pub async fn run_solo(app_config: &AppConfig) {
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
    let summary = game::run_game(
        config,
        words,
        event_rx,
        bonus_sender,
        flash_tx,
        None,
        None,
        None,
    )
    .await;

    // Abort background tasks
    input_handle.abort();
    timer_handle.abort();

    // Render summary inside the alt screen and wait for any key.
    let term_size = render::terminal_size();
    render::render_game_summary(
        summary.score,
        summary.total_questions,
        &summary.missed_words,
        summary.game_time,
        summary.all_used,
        &["Press any key to return to the main menu"],
        term_size,
    );

    let mut reader = EventStream::new();
    while let Some(Ok(event)) = reader.next().await {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                if input::is_ctrl_c(&key) {
                    render::force_exit();
                }
                break;
            }
        }
    }

    drop(_guard);
}

pub async fn run_host(app_config: &mut AppConfig, relay_addr: &str) {
    if let Err(e) = lobby::run_host_session(relay_addr, app_config).await {
        show_error(&format!("{}", e)).await;
    }
}

pub async fn run_join(conn: NetConnection, room_code: &str, my_id: u8) {
    if let Err(e) = lobby::run_joiner_session(conn, room_code, my_id).await {
        show_error(&format!("{}", e)).await;
    }
}

pub async fn show_error(msg: &str) {
    let _guard = render::TerminalGuard::new();
    let term_size = render::terminal_size();
    render::render_error(msg, term_size);

    // Wait for any keypress
    let mut reader = EventStream::new();
    while let Some(Ok(event)) = reader.next().await {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                if input::is_ctrl_c(&key) {
                    render::force_exit();
                }
                break;
            }
        }
    }
}
