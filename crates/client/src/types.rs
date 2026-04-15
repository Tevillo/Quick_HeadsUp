use protocol::{FlashKind, NetGameResult, NetUserAction};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum GameEvent {
    // Existing — solo + host
    UserInput(UserAction),
    TimerTick(u64),
    TimerExpired,
    Redraw,

    // Network events (received from remote peer)
    RemoteInput(UserAction),
    NetWordUpdate(String),
    NetTimerSync(u64),
    NetScoreUpdate { score: usize, total: usize },
    NetFlash(FlashKind),
    NetTimerExpired,
    NetGameOver(NetGameResult),
    PeerDisconnected,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserAction {
    Correct,
    Pass,
    Quit,
}

impl From<NetUserAction> for UserAction {
    fn from(a: NetUserAction) -> Self {
        match a {
            NetUserAction::Correct => UserAction::Correct,
            NetUserAction::Pass => UserAction::Pass,
            NetUserAction::Quit => UserAction::Quit,
        }
    }
}

impl From<UserAction> for NetUserAction {
    fn from(a: UserAction) -> Self {
        match a {
            UserAction::Correct => NetUserAction::Correct,
            UserAction::Pass => NetUserAction::Pass,
            UserAction::Quit => NetUserAction::Quit,
        }
    }
}

#[derive(Debug, Clone)]
pub enum GameMode {
    Normal,
    ExtraTime { bonus_seconds: u64 },
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub game_time: u64,
    pub skip_countdown: bool,
    pub last_unlimited: bool,
    pub mode: GameMode,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameResult {
    pub date: String,
    pub score: usize,
    pub total_questions: usize,
    pub missed_words: Vec<String>,
    pub game_time: u64,
    pub mode: String,
}

pub type EventSender = mpsc::Sender<GameEvent>;
pub type EventReceiver = mpsc::Receiver<GameEvent>;
pub type BonusSender = mpsc::Sender<u64>;
pub type BonusReceiver = mpsc::Receiver<u64>;
