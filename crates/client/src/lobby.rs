use crate::config::AppConfig;
use crate::game;
use crate::input;
use crate::net::{self, NetConnection};
use crate::render;
use crate::timer;
use crate::types::*;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use protocol::{ClientMessage, GameMessage, NetGameConfig, RelayMessage, Role};
use std::io::{self, ErrorKind};

// ─── Host session ────────────────────────────────────────────────────

pub async fn run_host_session(
    config: GameConfig,
    words: Vec<String>,
    relay_addr: &str,
    app_config: &AppConfig,
) -> io::Result<()> {
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
    let term_size = render::terminal_size();
    render::render_waiting_for_peer(&code, term_size);

    // Wait for peer to join
    loop {
        match conn.recv_relay_msg().await? {
            Some(RelayMessage::PeerJoined) => break,
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

    // Role selection
    let mut host_role = select_role().await;

    loop {
        // Ensure we're in alternate screen
        if guard.is_none() {
            guard = Some(render::TerminalGuard::new());
        }

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

        // --- Run the game (net tasks own the connection during this phase) ---
        let current_words =
            crate::load_words(&app_config.word_file, app_config.category.as_deref());
        let (summary, recovered_conn) =
            run_host_game(&config, current_words, conn, host_role).await;

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

pub async fn run_joiner_session(
    relay_addr: &str,
    code: &str,
    _app_config: &AppConfig,
) -> io::Result<()> {
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
        Some(RelayMessage::JoinedRoom) => {}
        Some(RelayMessage::Error(e)) => {
            return Err(io::Error::other(format!("Relay error: {}", e)));
        }
        _ => {
            return Err(io::Error::other("Unexpected relay response"));
        }
    }

    let _guard = render::TerminalGuard::new();
    let term_size = render::terminal_size();
    render::render_joined_room(&code_upper, term_size);

    // Wait for role assignment
    let my_role = loop {
        match conn.recv_relay_msg().await? {
            Some(RelayMessage::GameData(GameMessage::RoleAssignment { host_role })) => {
                let role = match host_role {
                    Role::Viewer => Role::Holder,
                    Role::Holder => Role::Viewer,
                };
                conn.send_client_msg(&ClientMessage::GameData(GameMessage::RoleAccepted))
                    .await?;
                render::render_role_assigned(role, term_size);
                break role;
            }
            Some(RelayMessage::Ping) => {
                conn.send_client_msg(&ClientMessage::Pong).await?;
            }
            Some(RelayMessage::PeerDisconnected) | None => {
                render::render_message("Host disconnected", term_size);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                return Ok(());
            }
            _ => {}
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
                render::render_message("Host disconnected", term_size);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                return Ok(());
            }
            _ => {}
        }
    }

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Run remote game
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<GameEvent>(32);
    let flash_tx = event_tx.clone();
    let input_tx = event_tx.clone();

    let net_handle = net::spawn_net_tasks(conn, event_tx);
    let net_tx = net_handle.outbound_tx.clone();

    let input_handle = tokio::spawn(input::input_task(input_tx));

    let summary = game::run_remote_game(my_role, event_rx, net_tx, flash_tx).await;

    input_handle.abort();
    let _ = net_handle.shutdown().await;

    drop(_guard);
    print_summary(&summary);

    Ok(())
}

// ─── Role selection (host only) ──────────────────────────────────────

async fn select_role() -> Role {
    let term_size = render::terminal_size();
    render::render_role_select(term_size);

    let mut reader = EventStream::new();
    loop {
        if let Some(Ok(Event::Key(key))) = reader.next().await {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('v') | KeyCode::Char('V') => return Role::Viewer,
                KeyCode::Char('h') | KeyCode::Char('H') => return Role::Holder,
                _ => {}
            }
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
                            return wait_for_peer_post_game(conn).await;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            conn.send_client_msg(&ClientMessage::GameData(GameMessage::SwapRoles)).await?;
                            return wait_for_peer_post_game(conn).await;
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

async fn wait_for_peer_post_game(conn: &mut NetConnection) -> io::Result<PostGameAction> {
    let term_size = render::terminal_size();
    render::render_message("Waiting for opponent...", term_size);

    loop {
        match conn.recv_relay_msg().await? {
            Some(RelayMessage::GameData(GameMessage::PlayAgain)) => {
                return Ok(PostGameAction::PlayAgain);
            }
            Some(RelayMessage::GameData(GameMessage::SwapRoles)) => {
                return Ok(PostGameAction::SwapRoles);
            }
            Some(RelayMessage::GameData(GameMessage::QuitSession))
            | Some(RelayMessage::PeerDisconnected)
            | None => {
                return Ok(PostGameAction::Quit);
            }
            Some(RelayMessage::Ping) => {
                conn.send_client_msg(&ClientMessage::Pong).await?;
            }
            _ => {}
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
