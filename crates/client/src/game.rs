use crate::net::OutboundMsg;
use crate::render;
use crate::types::*;
use chrono::Local;
use protocol::{FlashKind, GameMessage, NetGameResult, PeerId, Role};
use rand::seq::SliceRandom;
use std::fs;
use tokio::sync::mpsc;

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

/// Run the host (authoritative) game loop.
///
/// - `net_tx`: if Some, send OutboundMsgs to remote peers
/// - `local_role`: if Some, we're in networked mode. None = solo.
/// - `holder_peer_id`: identifies which remote peer is the Holder.
///   None if host is Holder or solo mode.
///
/// In networked mode, input routing depends on role:
/// - Viewer (local) → holder is remote → process `RemoteInput` from holder
/// - Holder (local) → holder is local → process `UserInput`
#[allow(clippy::too_many_arguments)]
pub async fn run_game(
    config: GameConfig,
    words: Vec<String>,
    mut rx: EventReceiver,
    bonus_tx: Option<BonusSender>,
    flash_tx: EventSender,
    net_tx: Option<mpsc::Sender<OutboundMsg>>,
    local_role: Option<Role>,
    holder_peer_id: Option<PeerId>,
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

    // Track whether a flash animation is playing to avoid clobbering it
    let mut flashing = false;

    // Render initial state based on role
    render_for_role(first_word, &state, local_role);

    // Send initial word to remote if networked
    if let Some(ref tx) = net_tx {
        let _ = tx
            .send(OutboundMsg::Broadcast(GameMessage::WordUpdate {
                word: first_word.to_string(),
            }))
            .await;
        let _ = tx
            .send(OutboundMsg::Broadcast(GameMessage::TimerSync {
                seconds_left: state.seconds_left,
            }))
            .await;
    }

    while let Some(event) = rx.recv().await {
        match event {
            // Route input based on role
            GameEvent::UserInput(action) | GameEvent::RemoteInput(_, action) => {
                // Determine if this input should be processed
                let should_process = match (local_role, &event) {
                    // Solo mode: process all UserInput
                    (None, GameEvent::UserInput(_)) => true,
                    // Networked, host is Viewer: holder is remote, process RemoteInput from holder
                    (Some(Role::Viewer), GameEvent::RemoteInput(pid, _))
                        if Some(*pid) == holder_peer_id =>
                    {
                        true
                    }
                    // Networked, host is Holder: process local UserInput
                    (Some(Role::Holder), GameEvent::UserInput(_)) => true,
                    // Networked, local is Viewer: forward local quit but ignore y/n
                    (Some(Role::Viewer), GameEvent::UserInput(UserAction::Quit)) => true,
                    _ => false,
                };

                if !should_process {
                    continue;
                }

                let current = match state.current_word() {
                    Some(w) => w.to_string(),
                    None => break,
                };

                match action {
                    UserAction::Correct => {
                        state.score += 1;
                        flashing = true;
                        render::flash_correct(flash_tx.clone());
                        if let Some(ref tx) = net_tx {
                            let _ = tx
                                .send(OutboundMsg::Broadcast(GameMessage::Flash(
                                    FlashKind::Correct,
                                )))
                                .await;
                        }
                        if let (Some(ref tx), GameMode::ExtraTime { bonus_seconds }) =
                            (&bonus_tx, &config.mode)
                        {
                            let _ = tx.send(*bonus_seconds).await;
                        }
                    }
                    UserAction::Pass => {
                        state.missed_words.push(current);
                        flashing = true;
                        render::flash_incorrect(flash_tx.clone());
                        if let Some(ref tx) = net_tx {
                            let _ = tx
                                .send(OutboundMsg::Broadcast(GameMessage::Flash(
                                    FlashKind::Incorrect,
                                )))
                                .await;
                        }
                    }
                    UserAction::Quit => break,
                }

                state.advance_word();

                // Send score update to remote
                if let Some(ref tx) = net_tx {
                    let _ = tx
                        .send(OutboundMsg::Broadcast(GameMessage::ScoreUpdate {
                            score: state.score,
                            total: state.total_questions,
                        }))
                        .await;
                }

                match state.current_word() {
                    Some(word) => {
                        if !flashing {
                            render_for_role(word, &state, local_role);
                        }
                        if let Some(ref tx) = net_tx {
                            let _ = tx
                                .send(OutboundMsg::Broadcast(GameMessage::WordUpdate {
                                    word: word.to_string(),
                                }))
                                .await;
                        }
                    }
                    None => {
                        // All words exhausted
                        break;
                    }
                }
            }
            GameEvent::TimerTick(remaining) => {
                state.seconds_left = remaining;
                if !flashing {
                    if let Some(word) = state.current_word() {
                        render_for_role(word, &state, local_role);
                    }
                }
                if let Some(ref tx) = net_tx {
                    let _ = tx
                        .send(OutboundMsg::Broadcast(GameMessage::TimerSync {
                            seconds_left: remaining,
                        }))
                        .await;
                }
            }
            GameEvent::TimerExpired => {
                render::bell();
                if let Some(ref tx) = net_tx {
                    let _ = tx
                        .send(OutboundMsg::Broadcast(GameMessage::TimerExpired))
                        .await;
                }
                if config.last_unlimited {
                    if let Some(word) = state.current_word() {
                        render::render_question_unlimited(word, state.score, state.term_size);
                        while let Some(evt) = rx.recv().await {
                            let action = match (&evt, local_role) {
                                (GameEvent::UserInput(a), None | Some(Role::Holder)) => Some(*a),
                                (GameEvent::RemoteInput(pid, a), Some(Role::Viewer))
                                    if Some(*pid) == holder_peer_id =>
                                {
                                    Some(*a)
                                }
                                _ => None,
                            };
                            if let Some(action) = action {
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
                    if let Some(word) = state.current_word() {
                        state.missed_words.push(word.to_string());
                        state.total_questions += 1;
                    }
                    break;
                }
            }
            GameEvent::Redraw => {
                flashing = false;
                if let Some(word) = state.current_word() {
                    render_for_role(word, &state, local_role);
                }
            }
            GameEvent::PeerDisconnected(pid) => {
                if Some(pid) == holder_peer_id {
                    // Holder disconnected — end the round
                    render::render_message("Holder disconnected", state.term_size);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    break;
                } else if local_role.is_some() && pid == 0 {
                    // Host disconnected (shouldn't happen in host game, but handle it)
                    render::render_message("Host disconnected", state.term_size);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    break;
                }
                // Viewer disconnected — non-fatal, continue playing
            }
            _ => {} // Ignore Net* events in host game loop (they're for remote render)
        }
    }

    let all_used = state.word_index >= state.words.len();

    // Send game over to remote
    if let Some(ref tx) = net_tx {
        let _ = tx
            .send(OutboundMsg::Broadcast(GameMessage::GameOver(
                NetGameResult {
                    score: state.score,
                    total_questions: state.total_questions,
                    missed_words: state.missed_words.clone(),
                    game_time: config.game_time,
                    all_used,
                },
            )))
            .await;
    }

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

/// Render based on role: Viewer sees the word, Holder sees "GUESS!"
fn render_for_role(word: &str, state: &GameState, local_role: Option<Role>) {
    match local_role {
        None | Some(Role::Viewer) => {
            render::render_question(word, state.seconds_left, state.score, state.term_size);
        }
        Some(Role::Holder) => {
            render::render_holder_view(state.seconds_left, state.score, state.term_size);
        }
    }
}

/// Run the remote (non-host) game loop. Receives display updates, forwards input.
pub async fn run_remote_game(
    role: Role,
    mut rx: EventReceiver,
    net_tx: mpsc::Sender<OutboundMsg>,
    flash_tx: EventSender,
) -> GameSummary {
    let term_size = render::terminal_size();
    let mut current_word = String::new();
    let mut seconds_left: u64 = 0;
    let mut score: usize = 0;
    let mut total: usize = 0;
    let mut flashing = false;

    loop {
        let Some(event) = rx.recv().await else {
            break;
        };

        match event {
            GameEvent::NetWordUpdate(word) => {
                current_word = word;
                if !flashing {
                    match role {
                        Role::Viewer => {
                            render::render_question(&current_word, seconds_left, score, term_size);
                        }
                        Role::Holder => {
                            render::render_holder_view(seconds_left, score, term_size);
                        }
                    }
                }
            }
            GameEvent::NetTimerSync(secs) => {
                seconds_left = secs;
                if !flashing {
                    match role {
                        Role::Viewer => {
                            render::render_question(&current_word, seconds_left, score, term_size);
                        }
                        Role::Holder => {
                            render::render_holder_view(seconds_left, score, term_size);
                        }
                    }
                }
            }
            GameEvent::NetScoreUpdate { score: s, total: t } => {
                score = s;
                total = t;
            }
            GameEvent::NetFlash(kind) => {
                flashing = true;
                match kind {
                    protocol::FlashKind::Correct => render::flash_correct(flash_tx.clone()),
                    protocol::FlashKind::Incorrect => render::flash_incorrect(flash_tx.clone()),
                }
            }
            GameEvent::NetTimerExpired => {
                render::bell();
            }
            GameEvent::NetGameOver(result) => {
                return GameSummary {
                    score: result.score,
                    total_questions: result.total_questions,
                    missed_words: result.missed_words,
                    game_time: result.game_time,
                    all_used: result.all_used,
                };
            }
            GameEvent::UserInput(action) => {
                // If we're the holder, forward input to host
                if role == Role::Holder {
                    let net_action: protocol::NetUserAction = action.into();
                    let _ = net_tx
                        .send(OutboundMsg::Broadcast(GameMessage::PlayerInput(net_action)))
                        .await;
                    if action == UserAction::Quit {
                        break;
                    }
                } else if action == UserAction::Quit {
                    let _ = net_tx
                        .send(OutboundMsg::Broadcast(GameMessage::PlayerInput(
                            protocol::NetUserAction::Quit,
                        )))
                        .await;
                    break;
                }
            }
            GameEvent::PeerDisconnected(pid) => {
                if pid == protocol::HOST_PEER_ID {
                    render::render_message("Host disconnected", term_size);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    break;
                }
                // Other peer disconnects: ignore (host handles it)
            }
            GameEvent::Redraw => {
                flashing = false;
                match role {
                    Role::Viewer => {
                        render::render_question(&current_word, seconds_left, score, term_size);
                    }
                    Role::Holder => {
                        render::render_holder_view(seconds_left, score, term_size);
                    }
                }
            }
            _ => {}
        }
    }

    GameSummary {
        score,
        total_questions: total,
        missed_words: Vec::new(),
        game_time: 0,
        all_used: false,
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
    let path = home.join(".guess_up_history.json");

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
