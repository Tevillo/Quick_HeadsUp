use crate::render;
use crate::types::*;
use chrono::Local;
use rand::seq::SliceRandom;
use std::fs;

pub struct GameSummary {
    pub score: usize,
    pub total_questions: usize,
    pub missed_words: Vec<String>,
    pub game_time: u64,
    pub all_used: bool,
}

pub struct GameState {
    pub words: Vec<String>,
    pub word_index: usize,
    pub score: usize,
    pub total_questions: usize,
    pub missed_words: Vec<String>,
    pub seconds_left: u64,
    pub term_size: (u16, u16),
}

impl GameState {
    pub fn new(mut words: Vec<String>, game_time: u64, term_size: (u16, u16)) -> Self {
        let mut rng = rand::thread_rng();
        words.shuffle(&mut rng);
        GameState {
            words,
            word_index: 0,
            score: 0,
            total_questions: 0,
            missed_words: Vec::new(),
            seconds_left: game_time,
            term_size,
        }
    }

    pub fn current_word(&self) -> Option<&str> {
        self.words.get(self.word_index).map(|s| s.as_str())
    }

    fn advance_word(&mut self) {
        self.word_index += 1;
        self.total_questions += 1;
    }
}

pub async fn run_game(
    config: GameConfig,
    words: Vec<String>,
    mut rx: EventReceiver,
    bonus_tx: Option<BonusSender>,
    flash_tx: EventSender,
) -> GameSummary {
    let term_size = render::terminal_size();
    let mut state = GameState::new(words, config.game_time, term_size);

    let Some(first_word) = state.current_word() else {
        return GameSummary {
            score: 0,
            total_questions: 0,
            missed_words: Vec::new(),
            game_time: config.game_time,
            all_used: true,
        };
    };
    render::render_question(first_word, state.seconds_left, state.score, state.term_size);

    while let Some(event) = rx.recv().await {
        match event {
            GameEvent::UserInput(action) => {
                let current = match state.current_word() {
                    Some(w) => w.to_string(),
                    None => break,
                };

                match action {
                    UserAction::Correct => {
                        state.score += 1;
                        render::flash_correct(flash_tx.clone());
                        if let (Some(ref tx), GameMode::ExtraTime { bonus_seconds }) =
                            (&bonus_tx, &config.mode)
                        {
                            let _ = tx.send(*bonus_seconds).await;
                        }
                    }
                    UserAction::Pass => {
                        state.missed_words.push(current);
                        render::flash_incorrect(flash_tx.clone());
                    }
                    UserAction::Quit => break,
                }

                state.advance_word();
                match state.current_word() {
                    Some(word) => {
                        render::render_question(
                            word,
                            state.seconds_left,
                            state.score,
                            state.term_size,
                        );
                    }
                    None => {
                        // All words exhausted
                        break;
                    }
                }
            }
            GameEvent::TimerTick(remaining) => {
                state.seconds_left = remaining;
                if let Some(word) = state.current_word() {
                    render::render_question(word, state.seconds_left, state.score, state.term_size);
                }
            }
            GameEvent::TimerExpired => {
                render::bell();
                if config.last_unlimited {
                    // Show last question with no time limit
                    if let Some(word) = state.current_word() {
                        render::render_question_unlimited(word, state.score, state.term_size);
                        // Wait for one more input
                        while let Some(evt) = rx.recv().await {
                            if let GameEvent::UserInput(action) = evt {
                                let word = state.current_word().unwrap_or("").to_string();
                                match action {
                                    UserAction::Correct => state.score += 1,
                                    _ => state.missed_words.push(word),
                                }
                                state.total_questions += 1;
                                break;
                            }
                        }
                    }
                    break;
                } else {
                    // Current word counts as missed
                    if let Some(word) = state.current_word() {
                        state.missed_words.push(word.to_string());
                        state.total_questions += 1;
                    }
                    break;
                }
            }
            GameEvent::Redraw => {
                if let Some(word) = state.current_word() {
                    render::render_question(word, state.seconds_left, state.score, state.term_size);
                }
            }
        }
    }

    let all_used = state.word_index >= state.words.len();

    drop(flash_tx);
    save_history(&state, &config);

    GameSummary {
        score: state.score,
        total_questions: state.total_questions,
        missed_words: state.missed_words,
        game_time: config.game_time,
        all_used,
    }
}

fn save_history(state: &GameState, config: &GameConfig) {
    let result = GameResult {
        date: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        score: state.score,
        total_questions: state.total_questions,
        missed_words: state.missed_words.clone(),
        game_time: config.game_time,
        mode: match &config.mode {
            GameMode::Normal => "normal".to_string(),
            GameMode::ExtraTime { bonus_seconds } => {
                format!("extra-time (+{}s)", bonus_seconds)
            }
        },
    };

    let Some(home) = dirs::home_dir() else {
        return;
    };
    let path = home.join(".heads_up_history.json");

    let mut history: Vec<GameResult> = if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    };

    history.push(result);

    if let Ok(json) = serde_json::to_string_pretty(&history) {
        let _ = fs::write(&path, json);
    }
}
