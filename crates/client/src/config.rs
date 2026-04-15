use crate::types::{GameConfig, GameMode};
use serde::{Deserialize, Serialize};
use std::fs;

const CONFIG_FILENAME: &str = ".guess_up_config.json";
const MAX_RECENT_SERVERS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub game_time: u64,
    pub skip_countdown: bool,
    pub last_unlimited: bool,
    pub extra_time: bool,
    pub bonus_seconds: u64,
    pub word_file: String,
    pub category: Option<String>,
    pub recent_servers: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            game_time: 60,
            skip_countdown: false,
            last_unlimited: false,
            extra_time: false,
            bonus_seconds: 5,
            word_file: "files/ASOIAF_list.txt".to_string(),
            category: None,
            recent_servers: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let Some(home) = dirs::home_dir() else {
            return Self::default();
        };
        let path = home.join(CONFIG_FILENAME);
        match fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let path = home.join(CONFIG_FILENAME);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }

    pub fn to_game_config(&self) -> GameConfig {
        GameConfig {
            game_time: self.game_time,
            skip_countdown: self.skip_countdown,
            last_unlimited: self.last_unlimited,
            mode: if self.extra_time {
                GameMode::ExtraTime {
                    bonus_seconds: self.bonus_seconds,
                }
            } else {
                GameMode::Normal
            },
        }
    }

    /// Push a server address to the front of recent_servers, deduplicating.
    pub fn push_recent_server(&mut self, addr: &str) {
        self.recent_servers.retain(|s| s != addr);
        self.recent_servers.insert(0, addr.to_string());
        self.recent_servers.truncate(MAX_RECENT_SERVERS);
    }
}
