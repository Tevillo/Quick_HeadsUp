mod game;
mod input;
mod lobby;
mod net;
mod render;
mod timer;
mod types;

use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use types::*;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "ASOIAF Heads Up! — A Song of Ice and Fire themed party game"
)]
struct Args {
    /// Game length in seconds
    #[arg(short, long, default_value_t = 60)]
    game_time: u64,

    /// Skip the 3-2-1 countdown animation
    #[arg(short, long)]
    skip_countdown: bool,

    /// Allow unlimited time for the last question when timer expires
    #[arg(short, long)]
    last_unlimited: bool,

    /// Enable extra-time mode: correct answers add bonus seconds
    #[arg(short = 'x', long)]
    extra_time: bool,

    /// Seconds added per correct answer in extra-time mode
    #[arg(long, default_value_t = 5)]
    bonus_seconds: u64,

    /// Path to word list file
    #[arg(short, long, default_value = "files/ASOIAF_list.txt")]
    word_file: String,

    /// Filter to a specific category in the word file (e.g. "House Stark")
    #[arg(long)]
    category: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create a room and wait for an opponent
    Host {
        /// Relay server address (host:port)
        #[arg(long)]
        relay: String,
    },
    /// Join an existing room
    Join {
        /// Relay server address (host:port)
        #[arg(long)]
        relay: String,
        /// Room code to join
        #[arg(long)]
        code: String,
    },
}

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

#[tokio::main]
async fn main() {
    let mut args = Args::parse();
    let command = args.command.take();

    match command {
        None => run_solo(args).await,
        Some(Command::Host { relay }) => run_host(args, relay).await,
        Some(Command::Join { relay, code }) => run_join(args, relay, code).await,
    }
}

async fn run_solo(args: Args) {
    let words = load_words(&args.word_file, args.category.as_deref());
    if words.is_empty() {
        eprintln!(
            "No words found in '{}' (category: {:?})",
            args.word_file, args.category
        );
        std::process::exit(1);
    }

    let config = GameConfig {
        game_time: args.game_time,
        skip_countdown: args.skip_countdown,
        last_unlimited: args.last_unlimited,
        mode: if args.extra_time {
            GameMode::ExtraTime {
                bonus_seconds: args.bonus_seconds,
            }
        } else {
            GameMode::Normal
        },
    };

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
    let timer_handle = tokio::spawn(timer::timer_task(timer_tx, args.game_time, bonus_rx));

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

async fn run_host(args: Args, relay_addr: String) {
    let words = load_words(&args.word_file, args.category.as_deref());
    if words.is_empty() {
        eprintln!(
            "No words found in '{}' (category: {:?})",
            args.word_file, args.category
        );
        std::process::exit(1);
    }

    let config = GameConfig {
        game_time: args.game_time,
        skip_countdown: args.skip_countdown,
        last_unlimited: args.last_unlimited,
        mode: if args.extra_time {
            GameMode::ExtraTime {
                bonus_seconds: args.bonus_seconds,
            }
        } else {
            GameMode::Normal
        },
    };

    if let Err(e) = lobby::run_host_session(config, words, &relay_addr, &args).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run_join(args: Args, relay_addr: String, code: String) {
    if let Err(e) = lobby::run_joiner_session(&relay_addr, &code, &args).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
