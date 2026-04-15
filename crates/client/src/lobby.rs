use crate::config::AppConfig;
use crate::game;
use crate::input;
use crate::menu;
use crate::net::{self, NetConnection};
use crate::render::{self, MenuItem};
use crate::timer;
use crate::types::*;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use protocol::{ClientMessage, GameMessage, NetGameConfig, RelayMessage, Role};
use std::io::{self, ErrorKind};

// ─── Host session ────────────────────────────────────────────────────

pub async fn run_host_session(relay_addr: &str, app_config: &mut AppConfig) -> io::Result<()> {
    let mut conn = NetConnection::connect(relay_addr).await.map_err(|e| {
        io::Error::new(
            ErrorKind::ConnectionRefused,
            format!("Could not connect to relay at {}: {}", relay_addr, e),
        )
    })?;

    // Create room
    conn.send_client_msg(&ClientMessage::CreateRoom).await?;
    let code = match conn.recv_relay_msg().await? {
        Some(RelayMessage::RoomCreated { code }) => code,
        Some(RelayMessage::Error(e)) => {
            return Err(io::Error::other(format!("Relay error: {}", e)));
        }
        _ => {
            return Err(io::Error::other("Unexpected relay response"));
        }
    };

    let mut guard = Some(render::TerminalGuard::new());

    // Wait for peer (with settings access)
    let wait_result = wait_for_peer(&code, &mut conn, app_config).await?;
    if !wait_result {
        let _ = conn.send_client_msg(&ClientMessage::Disconnect).await;
        return Ok(());
    }

    // Role selection
    let mut host_role = select_role(app_config).await;

    loop {
        // Rebuild config and words from current settings each round
        let config = app_config.to_game_config();
        let words = crate::load_words(&app_config.word_file, app_config.category.as_deref());
        if words.is_empty() {
            let term_size = render::terminal_size();
            render::render_message("No words found for current settings!", term_size);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = conn.send_client_msg(&ClientMessage::Disconnect).await;
            return Ok(());
        }

        // Ensure we're in alternate screen
        if guard.is_none() {
            guard = Some(render::TerminalGuard::new());
        }

        let term_size = render::terminal_size();

        // Send role assignment to joiner
        conn.send_client_msg(&ClientMessage::GameData(GameMessage::RoleAssignment {
            host_role,
        }))
        .await?;

        // Wait for role accepted
        loop {
            match conn.recv_relay_msg().await? {
                Some(RelayMessage::GameData(GameMessage::RoleAccepted)) => break,
                Some(RelayMessage::Ping) => {
                    conn.send_client_msg(&ClientMessage::Pong).await?;
                }
                Some(RelayMessage::PeerDisconnected) | None => {
                    render::render_message("Opponent disconnected", term_size);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    return Ok(());
                }
                _ => {}
            }
        }

        // Send game config
        let net_config = NetGameConfig {
            game_time: config.game_time,
            last_unlimited: config.last_unlimited,
            extra_time: matches!(config.mode, GameMode::ExtraTime { .. }),
            bonus_seconds: match &config.mode {
                GameMode::ExtraTime { bonus_seconds } => *bonus_seconds,
                _ => 0,
            },
            word_count: words.len(),
        };
        conn.send_client_msg(&ClientMessage::GameData(GameMessage::GameStart(net_config)))
            .await?;

        render::render_role_assigned(host_role, term_size);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if !config.skip_countdown {
            render::render_countdown(term_size);
        }

        // --- Run the game ---
        let (summary, recovered_conn) = run_host_game(&config, words, conn, host_role).await;

        conn = match recovered_conn {
            Some(c) => c,
            None => {
                // Connection lost — show summary and exit
                guard.take(); // drop guard to leave alternate screen
                print_summary(&summary);
                return Ok(());
            }
        };

        // Show summary (temporarily leave alternate screen)
        guard.take();
        print_summary(&summary);

        // Re-enter alternate screen for post-game menu
        guard = Some(render::TerminalGuard::new());

        let post_action = run_post_game_menu(&mut conn).await?;
        match post_action {
            PostGameAction::PlayAgain => {}
            PostGameAction::SwapRoles => {
                host_role = match host_role {
                    Role::Viewer => Role::Holder,
                    Role::Holder => Role::Viewer,
                };
            }
            PostGameAction::Quit => {
                let _ = conn.send_client_msg(&ClientMessage::Disconnect).await;
                return Ok(());
            }
        }
    }
}

// ─── Wait for peer (with settings menu) ─────────────────────────────

/// Wait for a peer to join the room. Returns `true` if a peer joined,
/// `false` if the host chose to disconnect.
async fn wait_for_peer(
    room_code: &str,
    conn: &mut NetConnection,
    app_config: &mut AppConfig,
) -> io::Result<bool> {
    let mut reader = EventStream::new();
    let mut selected: usize = 0;
    let count = 2; // Settings, Disconnect

    loop {
        let term_size = render::terminal_size();
        let code_line = format!("Room: {}", room_code);
        let items = [
            MenuItem::Label("WAITING FOR OPPONENT..."),
            MenuItem::Label(""),
            MenuItem::Label(&code_line),
            MenuItem::Label(""),
            MenuItem::Action("Settings"),
            MenuItem::Action("Disconnect"),
        ];
        render::render_menu("HOST LOBBY", &items, selected, term_size);

        tokio::select! {
            event = reader.next() => {
                let Some(Ok(event)) = event else { continue };
                let Event::Key(key) = event else { continue };
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
                        0 => menu::run_settings_inline(app_config, &mut reader).await,
                        1 => return Ok(false),
                        _ => {}
                    },
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(false),
                    _ => {}
                }
            }
            msg = conn.recv_relay_msg() => {
                match msg? {
                    Some(RelayMessage::PeerJoined) => return Ok(true),
                    Some(RelayMessage::Ping) => {
                        conn.send_client_msg(&ClientMessage::Pong).await?;
                    }
                    None => {
                        return Err(io::Error::new(
                            ErrorKind::ConnectionAborted,
                            "Relay disconnected",
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
}

// ─── Host game ──────────────────────────────────────────────────────

async fn run_host_game(
    config: &GameConfig,
    words: Vec<String>,
    conn: NetConnection,
    local_role: Role,
) -> (game::GameSummary, Option<NetConnection>) {
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

    let net_handle = net::spawn_net_tasks(conn, event_tx);
    let net_tx = net_handle.outbound_tx.clone();

    let input_handle = tokio::spawn(input::input_task(input_tx));
    let timer_handle = tokio::spawn(timer::timer_task(timer_tx, config.game_time, bonus_rx));

    let summary = game::run_game(
        config.clone(),
        words,
        event_rx,
        bonus_sender,
        flash_tx,
        Some(net_tx),
        Some(local_role),
    )
    .await;

    input_handle.abort();
    timer_handle.abort();

    // Recover the connection from net tasks
    let recovered = net_handle.shutdown().await;

    (summary, recovered)
}

// ─── Joiner session ─────────────────────────────────────────────────

/// Connect to the relay and join a room. Returns the established connection
/// on success, or an error (e.g. bad room code, connection refused).
pub async fn try_join_room(relay_addr: &str, code: &str) -> io::Result<NetConnection> {
    let mut conn = NetConnection::connect(relay_addr).await.map_err(|e| {
        io::Error::new(
            ErrorKind::ConnectionRefused,
            format!("Could not connect to relay at {}: {}", relay_addr, e),
        )
    })?;

    let code_upper = code.to_uppercase();
    conn.send_client_msg(&ClientMessage::JoinRoom {
        code: code_upper.clone(),
    })
    .await?;

    match conn.recv_relay_msg().await? {
        Some(RelayMessage::JoinedRoom) => Ok(conn),
        Some(RelayMessage::Error(e)) => Err(io::Error::other(format!("{}", e))),
        _ => Err(io::Error::other("Unexpected relay response")),
    }
}

/// Run the joiner game session on an already-joined connection.
/// Loops across games until the host quits or disconnects.
pub async fn run_joiner_session(mut conn: NetConnection, room_code: &str) -> io::Result<()> {
    let mut guard = Some(render::TerminalGuard::new());
    let term_size = render::terminal_size();
    render::render_joined_room(room_code, term_size);
    let mut waiting_for_next_round = false;

    loop {
        // Ensure we're in alternate screen
        if guard.is_none() {
            guard = Some(render::TerminalGuard::new());
        }

        // Wait for role assignment from host. Between rounds the joiner
        // can press Q to quit. We also accept (and ignore) PlayAgain /
        // SwapRoles messages — only RoleAssignment starts the next round.
        let my_role = {
            let msg = if waiting_for_next_round {
                "Waiting for host..."
            } else {
                "Waiting for host to choose roles..."
            };
            let term_size = render::terminal_size();
            render::render_message(msg, term_size);

            let mut reader = EventStream::new();
            loop {
                tokio::select! {
                    event = reader.next() => {
                        if let Some(Ok(Event::Key(key))) = event {
                            if key.kind == KeyEventKind::Press
                                && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc)
                            {
                                let _ = conn
                                    .send_client_msg(&ClientMessage::GameData(
                                        GameMessage::QuitSession,
                                    ))
                                    .await;
                                return Ok(());
                            }
                        }
                    }
                    msg = conn.recv_relay_msg() => {
                        match msg? {
                            Some(RelayMessage::GameData(GameMessage::RoleAssignment {
                                host_role,
                            })) => {
                                let role = match host_role {
                                    Role::Viewer => Role::Holder,
                                    Role::Holder => Role::Viewer,
                                };
                                conn.send_client_msg(&ClientMessage::GameData(
                                    GameMessage::RoleAccepted,
                                ))
                                .await?;
                                let term_size = render::terminal_size();
                                render::render_role_assigned(role, term_size);
                                break role;
                            }
                            Some(RelayMessage::Ping) => {
                                conn.send_client_msg(&ClientMessage::Pong).await?;
                            }
                            Some(RelayMessage::PeerDisconnected) | None => {
                                let term_size = render::terminal_size();
                                render::render_message("Host disconnected", term_size);
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                return Ok(());
                            }
                            Some(RelayMessage::GameData(GameMessage::QuitSession)) => {
                                return Ok(());
                            }
                            // Ignore PlayAgain/SwapRoles — we only care about RoleAssignment
                            _ => {}
                        }
                    }
                }
            }
        };

        // Wait for game start
        loop {
            match conn.recv_relay_msg().await? {
                Some(RelayMessage::GameData(GameMessage::GameStart(_cfg))) => break,
                Some(RelayMessage::Ping) => {
                    conn.send_client_msg(&ClientMessage::Pong).await?;
                }
                Some(RelayMessage::PeerDisconnected) | None => {
                    let term_size = render::terminal_size();
                    render::render_message("Host disconnected", term_size);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    return Ok(());
                }
                _ => {}
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Run remote game, recovering the connection afterward
        let (summary, recovered_conn) = run_joiner_game(my_role, conn).await;

        conn = match recovered_conn {
            Some(c) => c,
            None => {
                guard.take();
                print_summary(&summary);
                return Ok(());
            }
        };

        // Show summary (temporarily leave alternate screen)
        guard.take();
        print_summary(&summary);

        // Next iteration will re-enter alt screen and wait for RoleAssignment
        waiting_for_next_round = true;
    }
}

async fn run_joiner_game(
    role: Role,
    conn: NetConnection,
) -> (game::GameSummary, Option<NetConnection>) {
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<GameEvent>(32);
    let flash_tx = event_tx.clone();
    let input_tx = event_tx.clone();

    let net_handle = net::spawn_net_tasks(conn, event_tx);
    let net_tx = net_handle.outbound_tx.clone();

    let input_handle = tokio::spawn(input::input_task(input_tx));

    let summary = game::run_remote_game(role, event_rx, net_tx, flash_tx).await;

    input_handle.abort();

    let recovered = net_handle.shutdown().await;
    (summary, recovered)
}

// ─── Role selection (host only) ──────────────────────────────────────

async fn select_role(app_config: &mut AppConfig) -> Role {
    let mut selected: usize = 0;
    let mut reader = EventStream::new();
    let count = 3; // Viewer, Holder, Settings

    loop {
        let term_size = render::terminal_size();
        let items = [
            MenuItem::Action("Viewer — See words, give clues"),
            MenuItem::Action("Holder — Guess and press Y/N"),
            MenuItem::Label(""),
            MenuItem::Action("Settings"),
        ];
        render::render_menu("CHOOSE YOUR ROLE", &items, selected, term_size);

        let Some(Ok(event)) = reader.next().await else {
            continue;
        };
        let Event::Key(key) = event else {
            continue;
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
                0 => return Role::Viewer,
                1 => return Role::Holder,
                2 => menu::run_settings_inline(app_config, &mut reader).await,
                _ => {}
            },
            KeyCode::Char('v') | KeyCode::Char('V') => return Role::Viewer,
            KeyCode::Char('h') | KeyCode::Char('H') => return Role::Holder,
            _ => {}
        }
    }
}

// ─── Post-game menu ──────────────────────────────────────────────────

enum PostGameAction {
    PlayAgain,
    SwapRoles,
    Quit,
}

async fn run_post_game_menu(conn: &mut NetConnection) -> io::Result<PostGameAction> {
    let term_size = render::terminal_size();
    render::render_post_game_menu(term_size);

    let mut reader = EventStream::new();
    loop {
        tokio::select! {
            event = reader.next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            conn.send_client_msg(&ClientMessage::GameData(GameMessage::PlayAgain)).await?;
                            return Ok(PostGameAction::PlayAgain);
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            conn.send_client_msg(&ClientMessage::GameData(GameMessage::SwapRoles)).await?;
                            return Ok(PostGameAction::SwapRoles);
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            conn.send_client_msg(&ClientMessage::GameData(GameMessage::QuitSession)).await?;
                            return Ok(PostGameAction::Quit);
                        }
                        _ => {}
                    }
                }
            }
            msg = conn.recv_relay_msg() => {
                match msg? {
                    Some(RelayMessage::GameData(GameMessage::QuitSession)) |
                    Some(RelayMessage::PeerDisconnected) | None => {
                        return Ok(PostGameAction::Quit);
                    }
                    Some(RelayMessage::GameData(GameMessage::PlayAgain)) => {
                        return Ok(PostGameAction::PlayAgain);
                    }
                    Some(RelayMessage::GameData(GameMessage::SwapRoles)) => {
                        return Ok(PostGameAction::SwapRoles);
                    }
                    Some(RelayMessage::Ping) => {
                        conn.send_client_msg(&ClientMessage::Pong).await?;
                    }
                    _ => {}
                }
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn print_summary(summary: &game::GameSummary) {
    render::print_output(
        summary.score,
        summary.total_questions,
        &summary.missed_words,
        summary.game_time,
        summary.all_used,
    );
}
