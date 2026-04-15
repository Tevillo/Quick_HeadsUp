use crate::config::AppConfig;
use crate::render::{self, MenuItem};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;

pub enum MenuAction {
    Solo,
    Host {
        relay_addr: String,
    },
    Join {
        relay_addr: String,
        room_code: String,
    },
    Quit,
}

enum Screen {
    Main,
    Settings,
}

pub async fn menu_loop(config: &mut AppConfig) -> MenuAction {
    let mut screen = Screen::Main;
    let mut selected: usize = 0;

    let mut reader = EventStream::new();

    loop {
        let term_size = render::terminal_size();

        match &screen {
            Screen::Main => render_main_menu(selected, term_size),
            Screen::Settings => render_settings_menu(config, selected, term_size),
        }

        match &screen {
            Screen::Main => {
                let count = 5; // Solo, Host, Join, Settings, Quit
                let Some(Ok(event)) = reader.next().await else {
                    continue;
                };
                let Event::Key(key) = event else {
                    continue; // Resize or other events → re-render
                };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.checked_sub(1).unwrap_or(count - 1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1) % count;
                    }
                    KeyCode::Enter => match selected {
                        0 => return MenuAction::Solo,
                        1 => {
                            let result = run_server_connect(&mut *config, true, &mut reader).await;
                            match result {
                                ServerConnectResult::Selected(addr) => {
                                    config.push_recent_server(&addr);
                                    return MenuAction::Host { relay_addr: addr };
                                }
                                ServerConnectResult::Back => {
                                    selected = 1;
                                }
                            }
                            continue;
                        }
                        2 => {
                            let result = run_server_connect(&mut *config, false, &mut reader).await;
                            match result {
                                ServerConnectResult::Selected(addr) => {
                                    config.push_recent_server(&addr);
                                    let join_result = run_join_room(&addr, &mut reader).await;
                                    match join_result {
                                        JoinRoomResult::Joined(code) => {
                                            return MenuAction::Join {
                                                relay_addr: addr,
                                                room_code: code,
                                            };
                                        }
                                        JoinRoomResult::Back => {
                                            selected = 2;
                                        }
                                    }
                                    continue;
                                }
                                ServerConnectResult::Back => {
                                    selected = 2;
                                }
                            }
                            continue;
                        }
                        3 => {
                            screen = Screen::Settings;
                            selected = 0;
                            continue;
                        }
                        4 => return MenuAction::Quit,
                        _ => {}
                    },
                    KeyCode::Char('q') | KeyCode::Esc => return MenuAction::Quit,
                    _ => {}
                }
            }
            Screen::Settings => {
                let result = run_settings_screen(config, &mut selected, &mut reader).await;
                match result {
                    SettingsResult::Continue => continue,
                    SettingsResult::Back => {
                        screen = Screen::Main;
                        selected = 3; // return cursor to Settings
                        continue;
                    }
                    SettingsResult::OpenCategoryPicker => {
                        let categories = crate::load_categories(&config.word_file);
                        let pick = run_category_picker(&categories, config, &mut reader).await;
                        if let Some(cat) = pick {
                            config.category = cat;
                        }
                        screen = Screen::Settings;
                        // Keep selected on the same item
                        continue;
                    }
                }
            }
        }
    }
}

// ─── Main menu ──────────────────────────────────────────────────────

fn render_main_menu(selected: usize, term_size: (u16, u16)) {
    let items = [
        MenuItem::Action("Solo Game"),
        MenuItem::Action("Host Game"),
        MenuItem::Action("Join Game"),
        MenuItem::Action("Settings"),
        MenuItem::Action("Quit"),
    ];
    render::render_menu("ASOIAF HEADS UP!", &items, selected, term_size);
}

// ─── Settings ───────────────────────────────────────────────────────

enum SettingsResult {
    Continue,
    Back,
    OpenCategoryPicker,
}

// Settings items indexed as selectable items:
// 0: Game Time
// 1: Skip Countdown
// 2: Last Unlimited
// 3: Extra Time
// 4: Bonus Seconds
// 5: Word File
// 6: Category
// 7: Back
const SETTINGS_COUNT: usize = 8;

fn render_settings_menu(config: &AppConfig, selected: usize, term_size: (u16, u16)) {
    let game_time_val = format!("< {} >", config.game_time);
    let skip_val = if config.skip_countdown {
        "[x]".to_string()
    } else {
        "[ ]".to_string()
    };
    let last_val = if config.last_unlimited {
        "[x]".to_string()
    } else {
        "[ ]".to_string()
    };
    let extra_val = if config.extra_time {
        "[x]".to_string()
    } else {
        "[ ]".to_string()
    };
    let bonus_val = format!("< {} >", config.bonus_seconds);
    let word_file_val = config.word_file.clone();
    let cat_val = config.category.as_deref().unwrap_or("All").to_string();

    let items = [
        MenuItem::Setting {
            label: "Game Time:",
            value: &game_time_val,
        },
        MenuItem::Setting {
            label: "Skip Countdown:",
            value: &skip_val,
        },
        MenuItem::Setting {
            label: "Last Unlimited:",
            value: &last_val,
        },
        MenuItem::Setting {
            label: "Extra Time:",
            value: &extra_val,
        },
        MenuItem::Setting {
            label: "Bonus Seconds:",
            value: &bonus_val,
        },
        MenuItem::Setting {
            label: "Word File:",
            value: &word_file_val,
        },
        MenuItem::Setting {
            label: "Category:",
            value: &cat_val,
        },
        MenuItem::Action("Back"),
    ];
    render::render_menu("SETTINGS", &items, selected, term_size);
}

async fn run_settings_screen(
    config: &mut AppConfig,
    selected: &mut usize,
    reader: &mut EventStream,
) -> SettingsResult {
    if let Some(Ok(Event::Key(key))) = reader.next().await {
        if key.kind != KeyEventKind::Press {
            return SettingsResult::Continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.checked_sub(1).unwrap_or(SETTINGS_COUNT - 1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1) % SETTINGS_COUNT;
            }
            KeyCode::Left | KeyCode::Char('h') => match *selected {
                0 => config.game_time = config.game_time.saturating_sub(5).max(5),
                4 => config.bonus_seconds = config.bonus_seconds.saturating_sub(1).max(1),
                1 => config.skip_countdown = !config.skip_countdown,
                2 => config.last_unlimited = !config.last_unlimited,
                3 => config.extra_time = !config.extra_time,
                _ => {}
            },
            KeyCode::Right | KeyCode::Char('l') => match *selected {
                0 => config.game_time = (config.game_time + 5).min(300),
                4 => config.bonus_seconds = (config.bonus_seconds + 1).min(30),
                1 => config.skip_countdown = !config.skip_countdown,
                2 => config.last_unlimited = !config.last_unlimited,
                3 => config.extra_time = !config.extra_time,
                _ => {}
            },
            KeyCode::Enter => match *selected {
                0 => {
                    // Text input for exact game time
                    if let Some(val) =
                        run_text_input_for_number("Game Time (seconds):", config.game_time, reader)
                            .await
                    {
                        config.game_time = val.clamp(5, 300);
                    }
                }
                1 => config.skip_countdown = !config.skip_countdown,
                2 => config.last_unlimited = !config.last_unlimited,
                3 => config.extra_time = !config.extra_time,
                4 => {
                    if let Some(val) =
                        run_text_input_for_number("Bonus Seconds:", config.bonus_seconds, reader)
                            .await
                    {
                        config.bonus_seconds = val.clamp(1, 30);
                    }
                }
                5 => {
                    // Text input for word file path
                    if let Some(val) = run_text_input("Word File:", &config.word_file, reader).await
                    {
                        if !val.is_empty() {
                            config.word_file = val;
                        }
                    }
                }
                6 => return SettingsResult::OpenCategoryPicker,
                7 => return SettingsResult::Back,
                _ => {}
            },
            KeyCode::Esc | KeyCode::Char('q') => return SettingsResult::Back,
            _ => {}
        }
    }
    SettingsResult::Continue
}

// ─── Category picker ────────────────────────────────────────────────

async fn run_category_picker(
    categories: &[String],
    config: &AppConfig,
    reader: &mut EventStream,
) -> Option<Option<String>> {
    // Build list: "All" + each category
    let mut items: Vec<String> = vec!["All".to_string()];
    items.extend(categories.iter().cloned());

    // Find initial selection based on current config
    let mut selected: usize = match &config.category {
        None => 0,
        Some(cat) => items
            .iter()
            .position(|c| c.eq_ignore_ascii_case(cat))
            .unwrap_or(0),
    };
    let mut scroll_offset: usize = 0;
    let visible = 15usize.min(items.len());

    loop {
        // Adjust scroll to keep selection visible
        if selected < scroll_offset {
            scroll_offset = selected;
        } else if selected >= scroll_offset + visible {
            scroll_offset = selected - visible + 1;
        }

        let term_size = render::terminal_size();
        render::render_category_picker(&items, selected, scroll_offset, term_size);

        if let Some(Ok(Event::Key(key))) = reader.next().await {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.checked_sub(1).unwrap_or(items.len() - 1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1) % items.len();
                }
                KeyCode::Enter => {
                    if selected == 0 {
                        return Some(None); // "All"
                    } else {
                        return Some(Some(items[selected].clone()));
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return None; // Cancel, keep current
                }
                _ => {}
            }
        }
    }
}

// ─── Address validation ─────────────────────────────────────────────

fn validate_address(addr: &str) -> Result<(), &'static str> {
    if addr.is_empty() {
        return Err("Address cannot be empty");
    }
    let Some(colon_pos) = addr.rfind(':') else {
        return Err("Missing port — use host:port (e.g. 192.168.1.5:7878)");
    };
    let host = &addr[..colon_pos];
    let port_str = &addr[colon_pos + 1..];
    if host.is_empty() {
        return Err("Host cannot be empty");
    }
    if port_str.is_empty() {
        return Err("Port cannot be empty — use host:port (e.g. server:7878)");
    }
    match port_str.parse::<u16>() {
        Ok(0) => Err("Port must be between 1 and 65535"),
        Ok(_) => Ok(()),
        Err(_) => Err("Port must be a number (e.g. 7878)"),
    }
}

// ─── Server connect ─────────────────────────────────────────────────

enum ServerConnectResult {
    Selected(String),
    Back,
}

async fn run_server_connect(
    config: &mut AppConfig,
    hosting: bool,
    reader: &mut EventStream,
) -> ServerConnectResult {
    let mut input_buf = String::new();
    let mut editing = true;
    let mut error_msg: Option<&'static str> = None;
    let mut selected: usize = 0; // 0 = text input

    loop {
        // Recalculate selectable count each iteration (recent servers don't change
        // during this screen, but Settings is only present when hosting)
        let recent_count = config.recent_servers.len();
        // selectable: text input + recent servers + [Settings if hosting] + Back
        let settings_offset = if hosting { 1 } else { 0 };
        let total_selectable = 1 + recent_count + settings_offset + 1;

        let term_size = render::terminal_size();
        let title = if hosting {
            "HOST — RELAY SERVER"
        } else {
            "JOIN — RELAY SERVER"
        };

        let mut items: Vec<MenuItem> = Vec::new();
        items.push(MenuItem::TextInput {
            label: "Address:",
            value: &input_buf,
            editing: editing && selected == 0,
        });

        if let Some(err) = error_msg {
            items.push(MenuItem::Error(err));
        }

        if !config.recent_servers.is_empty() {
            items.push(MenuItem::Label(""));
            items.push(MenuItem::Label("Recent servers:"));
            for server in &config.recent_servers {
                items.push(MenuItem::Action(server));
            }
        }

        items.push(MenuItem::Label(""));
        if hosting {
            items.push(MenuItem::Action("Settings"));
        }
        items.push(MenuItem::Action("Back"));

        render::render_menu(title, &items, selected, term_size);

        if let Some(Ok(Event::Key(key))) = reader.next().await {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // If we're editing text input
            if editing && selected == 0 {
                match key.code {
                    KeyCode::Enter => {
                        let addr = input_buf.trim().to_string();
                        match validate_address(&addr) {
                            Ok(()) => return ServerConnectResult::Selected(addr),
                            Err(e) => {
                                error_msg = Some(e);
                            }
                        }
                    }
                    KeyCode::Esc => {
                        editing = false;
                        error_msg = None;
                    }
                    KeyCode::Backspace => {
                        input_buf.pop();
                        error_msg = None;
                    }
                    KeyCode::Char(c) => {
                        input_buf.push(c);
                        error_msg = None;
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        editing = false;
                        error_msg = None;
                        selected = (selected + 1) % total_selectable;
                    }
                    _ => {}
                }
                continue;
            }

            // Normal navigation
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.checked_sub(1).unwrap_or(total_selectable - 1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1) % total_selectable;
                }
                KeyCode::Enter => {
                    if selected == 0 {
                        // Activate text input editing
                        editing = true;
                    } else if selected == total_selectable - 1 {
                        // Back (always last)
                        return ServerConnectResult::Back;
                    } else if hosting && selected == total_selectable - 2 {
                        // Settings (second to last when hosting)
                        run_settings_inline(config, reader).await;
                    } else {
                        // Recent server
                        let server_idx = selected - 1;
                        if server_idx < config.recent_servers.len() {
                            return ServerConnectResult::Selected(
                                config.recent_servers[server_idx].clone(),
                            );
                        }
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return ServerConnectResult::Back;
                }
                _ => {}
            }
        }
    }
}

/// Run the full settings screen inline (reusable from any screen).
pub async fn run_settings_inline(config: &mut AppConfig, reader: &mut EventStream) {
    let mut selected: usize = 0;
    loop {
        let term_size = render::terminal_size();
        render_settings_menu(config, selected, term_size);

        let result = run_settings_screen(config, &mut selected, reader).await;
        match result {
            SettingsResult::Continue => continue,
            SettingsResult::Back => return,
            SettingsResult::OpenCategoryPicker => {
                let categories = crate::load_categories(&config.word_file);
                let pick = run_category_picker(&categories, config, reader).await;
                if let Some(cat) = pick {
                    config.category = cat;
                }
            }
        }
    }
}

// ─── Join room ──────────────────────────────────────────────────────

enum JoinRoomResult {
    Joined(String),
    Back,
}

async fn run_join_room(relay_addr: &str, reader: &mut EventStream) -> JoinRoomResult {
    let mut input_buf = String::new();
    let mut editing = true;
    let mut selected: usize = 0; // 0 = text input, 1 = Back

    loop {
        let term_size = render::terminal_size();
        let title_text = format!("JOIN — {}", relay_addr);

        let items = [
            MenuItem::TextInput {
                label: "Room Code:",
                value: &input_buf,
                editing: editing && selected == 0,
            },
            MenuItem::Label(""),
            MenuItem::Action("Back"),
        ];

        render::render_menu(&title_text, &items, selected, term_size);

        if let Some(Ok(Event::Key(key))) = reader.next().await {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if editing && selected == 0 {
                match key.code {
                    KeyCode::Enter => {
                        let code = input_buf.trim().to_uppercase();
                        if !code.is_empty() {
                            return JoinRoomResult::Joined(code);
                        }
                        editing = false;
                    }
                    KeyCode::Esc => {
                        editing = false;
                    }
                    KeyCode::Backspace => {
                        input_buf.pop();
                    }
                    KeyCode::Char(c) => {
                        input_buf.push(c);
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        editing = false;
                        selected = 1;
                    }
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = 0;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = 1;
                }
                KeyCode::Enter => {
                    if selected == 0 {
                        editing = true;
                    } else {
                        return JoinRoomResult::Back;
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return JoinRoomResult::Back;
                }
                _ => {}
            }
        }
    }
}

// ─── Text input helpers ─────────────────────────────────────────────

async fn run_text_input(prompt: &str, initial: &str, reader: &mut EventStream) -> Option<String> {
    let mut buf = initial.to_string();

    loop {
        let term_size = render::terminal_size();
        let items = [
            MenuItem::TextInput {
                label: prompt,
                value: &buf,
                editing: true,
            },
            MenuItem::Label(""),
            MenuItem::Label("Enter to confirm, Esc to cancel"),
        ];
        render::render_menu("INPUT", &items, 0, term_size);

        if let Some(Ok(Event::Key(key))) = reader.next().await {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Enter => return Some(buf),
                KeyCode::Esc => return None,
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                }
                _ => {}
            }
        }
    }
}

async fn run_text_input_for_number(
    prompt: &str,
    initial: u64,
    reader: &mut EventStream,
) -> Option<u64> {
    let result = run_text_input(prompt, &initial.to_string(), reader).await?;
    result.parse::<u64>().ok()
}
