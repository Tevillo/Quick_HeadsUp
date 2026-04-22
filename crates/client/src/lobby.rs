use crate::config::AppConfig;
use crate::game;
use crate::input;
use crate::list_menu::{self, ListKey};
use crate::menu;
use crate::net::{self, ConnectError, NetConnection};
use crate::render::{self, MenuItem};
use crate::timer;
use crate::types::*;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use protocol::{
    ClientMessage, GameMessage, NetGameConfig, PeerId, RelayMessage, Role, HOST_PEER_ID,
};
use std::io::{self, ErrorKind};

/// Map a `ConnectError` to an `io::Error` suitable for the upstream
/// `io::Result`-returning caller. IO failures keep the "Could not connect to
/// relay at X:Y:" prefix; handshake rejections pass through their clean
/// Display format so the user sees exactly why the relay refused them.
fn connect_error_to_io(addr: &str, err: ConnectError) -> io::Error {
    match err {
        ConnectError::Io(io_err) => io::Error::new(
            ErrorKind::ConnectionRefused,
            format!("Could not connect to relay at {}: {}", addr, io_err),
        ),
        other => io::Error::other(format!("{}", other)),
    }
}

// ─── Host session ────────────────────────────────────────────────────

pub async fn run_host_session(relay_addr: &str, app_config: &mut AppConfig) -> io::Result<()> {
    let mut conn = NetConnection::connect(relay_addr)
        .await
        .map_err(|e| connect_error_to_io(relay_addr, e))?;

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

    let _guard = render::TerminalGuard::new();

    // Wait for players (multi-viewer lobby)
    let mut peers = match wait_for_players(&code, &mut conn, app_config).await? {
        Some(peers) => peers,
        None => {
            let _ = conn.send_client_msg(&ClientMessage::Disconnect).await;
            return Ok(());
        }
    };

    // Holder selection
    let mut holder_id = select_holder(&peers, app_config).await;

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

        let term_size = render::terminal_size();

        // Send role assignment to all joiners (broadcast)
        conn.send_client_msg(&ClientMessage::GameData {
            msg: GameMessage::RoleAssignment { holder_id },
            target: None,
        })
        .await?;

        // Wait for role accepted from all joiners
        let mut accepted = std::collections::HashSet::new();
        loop {
            if accepted.len() >= peers.len() {
                break;
            }
            match conn.recv_relay_msg().await? {
                Some(RelayMessage::GameData {
                    msg: GameMessage::RoleAccepted,
                    from,
                }) => {
                    accepted.insert(from);
                }
                Some(RelayMessage::Ping) => {
                    conn.send_client_msg(&ClientMessage::Pong).await?;
                }
                Some(RelayMessage::PeerDisconnected { peer_id }) => {
                    peers.retain(|&p| p != peer_id);
                    accepted.remove(&peer_id);
                    if peers.is_empty() {
                        render::render_message("All players disconnected", term_size);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        return Ok(());
                    }
                    // If the holder disconnected, pick the host as holder
                    if peer_id == holder_id {
                        holder_id = HOST_PEER_ID;
                        // Re-send role assignment
                        conn.send_client_msg(&ClientMessage::GameData {
                            msg: GameMessage::RoleAssignment { holder_id },
                            target: None,
                        })
                        .await?;
                        accepted.clear();
                    }
                }
                Some(RelayMessage::PeerJoined { peer_id }) => {
                    // Late joiner during role assignment — add them but
                    // they'll need a role assignment too
                    peers.push(peer_id);
                    conn.send_client_msg(&ClientMessage::GameData {
                        msg: GameMessage::RoleAssignment { holder_id },
                        target: Some(peer_id),
                    })
                    .await?;
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
        conn.send_client_msg(&ClientMessage::GameData {
            msg: GameMessage::GameStart(net_config),
            target: None,
        })
        .await?;

        // Determine host's local role
        let host_role = if holder_id == HOST_PEER_ID {
            Role::Holder
        } else {
            Role::Viewer
        };

        render::render_role_assigned(host_role, term_size);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if !config.skip_countdown {
            render::render_countdown(term_size);
        }

        // holder_peer_id is Some(id) if a joiner is the holder, None if host is holder
        let holder_peer = if holder_id == HOST_PEER_ID {
            None
        } else {
            Some(holder_id)
        };

        // --- Run the game ---
        let (summary, recovered_conn) =
            run_host_game(&config, words, conn, host_role, holder_peer).await;

        conn = match recovered_conn {
            Some(c) => c,
            None => {
                // Connection lost — show summary inside the alt screen and
                // wait for any key before returning to the main menu.
                show_summary_until_keypress(
                    &summary,
                    &["Connection to relay lost", "Press any key to continue..."],
                )
                .await;
                return Ok(());
            }
        };

        let post_action = run_post_game_menu(&mut conn, &mut peers, &summary).await?;
        match post_action {
            PostGameAction::PlayAgain => {}
            PostGameAction::PickNextHolder => {
                holder_id = select_holder(&peers, app_config).await;
            }
            PostGameAction::Quit => {
                let _ = conn.send_client_msg(&ClientMessage::Disconnect).await;
                return Ok(());
            }
        }
    }
}

// ─── Wait for players (multi-viewer lobby) ──────────────────────────

/// Wait for players to join the room. Returns `Some(Vec<PeerId>)` when
/// the host starts the game, or `None` if the host disconnects.
async fn wait_for_players(
    room_code: &str,
    conn: &mut NetConnection,
    app_config: &mut AppConfig,
) -> io::Result<Option<Vec<PeerId>>> {
    let mut reader = EventStream::new();
    let mut selected: usize = 0;
    let mut peers: Vec<PeerId> = Vec::new();

    loop {
        let has_players = !peers.is_empty();
        let count = if has_players { 3 } else { 2 }; // Start (if players), Settings, Disconnect

        let term_size = render::terminal_size();
        let code_line = format!("Room: {}", room_code);
        let player_count = format!("Players: {}/9", peers.len() + 1);

        let mut items: Vec<MenuItem> = vec![
            MenuItem::Label(&code_line),
            MenuItem::Label(""),
            MenuItem::Label(&player_count),
            MenuItem::Label("  Host (you)"),
        ];
        // Build player labels for each peer
        let peer_labels: Vec<String> = peers
            .iter()
            .map(|pid| format!("  Player {}", pid))
            .collect();
        for label in &peer_labels {
            items.push(MenuItem::Label(label));
        }
        items.push(MenuItem::Label(""));
        if has_players {
            items.push(MenuItem::Action("Start Game"));
        }
        items.push(MenuItem::Action("Settings"));
        items.push(MenuItem::Action("Disconnect"));

        render::render_menu("HOST LOBBY", &items, selected, term_size);

        tokio::select! {
            event = reader.next() => {
                let Some(Ok(event)) = event else { continue };
                let Event::Key(key) = event else { continue };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if crate::input::is_ctrl_c(&key) {
                    crate::render::force_exit();
                }
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.checked_sub(1).unwrap_or(count - 1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1) % count;
                    }
                    KeyCode::Enter => {
                        if has_players {
                            match selected {
                                0 => return Ok(Some(peers)),      // Start Game
                                1 => menu::run_settings_inline(app_config, &mut reader).await,
                                2 => return Ok(None),             // Disconnect
                                _ => {}
                            }
                        } else {
                            match selected {
                                0 => menu::run_settings_inline(app_config, &mut reader).await,
                                1 => return Ok(None),             // Disconnect
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    _ => {}
                }
            }
            msg = conn.recv_relay_msg() => {
                match msg? {
                    Some(RelayMessage::PeerJoined { peer_id }) => {
                        peers.push(peer_id);
                        // Reset selection to clamp it
                        let new_count = if !peers.is_empty() { 3 } else { 2 };
                        if selected >= new_count {
                            selected = new_count - 1;
                        }
                    }
                    Some(RelayMessage::PeerDisconnected { peer_id }) => {
                        peers.retain(|&p| p != peer_id);
                        let new_count = if !peers.is_empty() { 3 } else { 2 };
                        if selected >= new_count {
                            selected = new_count - 1;
                        }
                    }
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
    holder_peer_id: Option<PeerId>,
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
        holder_peer_id,
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
/// and the joiner's peer ID on success.
pub async fn try_join_room(relay_addr: &str, code: &str) -> io::Result<(NetConnection, PeerId)> {
    let mut conn = NetConnection::connect(relay_addr)
        .await
        .map_err(|e| connect_error_to_io(relay_addr, e))?;

    let code_upper = code.to_uppercase();
    conn.send_client_msg(&ClientMessage::JoinRoom {
        code: code_upper.clone(),
    })
    .await?;

    let peer_id = match conn.recv_relay_msg().await? {
        Some(RelayMessage::JoinedRoom { peer_id }) => peer_id,
        Some(RelayMessage::Error(e)) => return Err(io::Error::other(format!("{}", e))),
        _ => return Err(io::Error::other("Unexpected relay response")),
    };

    // Read and discard the PeerList — the joiner doesn't need it
    // since the host manages the participant list
    if let Some(RelayMessage::PeerList { .. }) = conn.recv_relay_msg().await? {
        // Expected — consumed and discarded
    }

    Ok((conn, peer_id))
}

/// Run the joiner game session on an already-joined connection.
/// Loops across games until the host quits or disconnects.
pub async fn run_joiner_session(
    mut conn: NetConnection,
    room_code: &str,
    my_id: PeerId,
) -> io::Result<()> {
    let _guard = render::TerminalGuard::new();
    let term_size = render::terminal_size();
    render::render_joined_room(room_code, term_size);
    let mut last_summary: Option<game::GameSummary> = None;

    loop {
        // Wait for role assignment from host. Between rounds the joiner
        // can press Q to quit. We also accept (and ignore) PlayAgain /
        // PickNextHolder messages — only RoleAssignment starts the next round.
        let my_role = {
            let term_size = render::terminal_size();
            if let Some(summary) = &last_summary {
                render::render_game_summary(
                    summary.score,
                    summary.total_questions,
                    &summary.missed_words,
                    summary.game_time,
                    summary.all_used,
                    &[
                        "Waiting for host to start the next round...",
                        "[Q] Quit session",
                    ],
                    term_size,
                );
            } else {
                render::render_message("Waiting for host to assign roles...", term_size);
            }

            let mut reader = EventStream::new();
            loop {
                tokio::select! {
                    event = reader.next() => {
                        if let Some(Ok(Event::Key(key))) = event {
                            if key.kind == KeyEventKind::Press && crate::input::is_ctrl_c(&key) {
                                crate::render::force_exit();
                            }
                            if key.kind == KeyEventKind::Press
                                && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc)
                            {
                                let _ = conn
                                    .send_client_msg(&ClientMessage::GameData {
                                        msg: GameMessage::QuitSession,
                                        target: None,
                                    })
                                    .await;
                                return Ok(());
                            }
                        }
                    }
                    msg = conn.recv_relay_msg() => {
                        match msg? {
                            Some(RelayMessage::GameData {
                                msg: GameMessage::RoleAssignment { holder_id },
                                ..
                            }) => {
                                let role = if holder_id == my_id {
                                    Role::Holder
                                } else {
                                    Role::Viewer
                                };
                                conn.send_client_msg(&ClientMessage::GameData {
                                    msg: GameMessage::RoleAccepted,
                                    target: None,
                                })
                                .await?;
                                let term_size = render::terminal_size();
                                render::render_role_assigned(role, term_size);
                                break role;
                            }
                            Some(RelayMessage::Ping) => {
                                conn.send_client_msg(&ClientMessage::Pong).await?;
                            }
                            Some(RelayMessage::PeerDisconnected { peer_id })
                                if peer_id == HOST_PEER_ID =>
                            {
                                let term_size = render::terminal_size();
                                render::render_message("Host disconnected", term_size);
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                return Ok(());
                            }
                            None => {
                                let term_size = render::terminal_size();
                                render::render_message("Host disconnected", term_size);
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                return Ok(());
                            }
                            Some(RelayMessage::GameData {
                                msg: GameMessage::QuitSession,
                                ..
                            }) => {
                                return Ok(());
                            }
                            // Ignore PlayAgain/PickNextHolder/PeerJoined/PeerDisconnected (non-host)
                            _ => {}
                        }
                    }
                }
            }
        };

        // Wait for game start
        loop {
            match conn.recv_relay_msg().await? {
                Some(RelayMessage::GameData {
                    msg: GameMessage::GameStart(_cfg),
                    ..
                }) => break,
                Some(RelayMessage::Ping) => {
                    conn.send_client_msg(&ClientMessage::Pong).await?;
                }
                Some(RelayMessage::PeerDisconnected { peer_id }) if peer_id == HOST_PEER_ID => {
                    let term_size = render::terminal_size();
                    render::render_message("Host disconnected", term_size);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    return Ok(());
                }
                None => {
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
                show_summary_until_keypress(
                    &summary,
                    &["Connection to host lost", "Press any key to continue..."],
                )
                .await;
                return Ok(());
            }
        };

        // Stay in the alt screen; the next iteration renders the summary as
        // the backdrop while we wait for the host to kick off the next round.
        last_summary = Some(summary);
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

// ─── Holder selection (host only) ───────────────────────────────────

async fn select_holder(peers: &[PeerId], app_config: &mut AppConfig) -> PeerId {
    let mut selected: usize = 0;
    let mut reader = EventStream::new();

    // Build participant list: Host + all peers
    let participant_count = 1 + peers.len(); // Host + peers
    let total_selectable = participant_count + 1; // participants + Settings

    loop {
        let term_size = render::terminal_size();

        let mut items: Vec<MenuItem> = Vec::new();
        items.push(MenuItem::Action("Host (you)"));
        let peer_labels: Vec<String> = peers.iter().map(|pid| format!("Player {}", pid)).collect();
        for label in &peer_labels {
            items.push(MenuItem::Action(label));
        }
        items.push(MenuItem::Label(""));
        items.push(MenuItem::Action("Settings"));

        render::render_menu("CHOOSE THE HOLDER", &items, selected, term_size);

        let Some(Ok(event)) = reader.next().await else {
            continue;
        };
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if crate::input::is_ctrl_c(&key) {
            crate::render::force_exit();
        }
        match list_menu::classify_key(&key) {
            ListKey::Up => {
                selected = selected.checked_sub(1).unwrap_or(total_selectable - 1);
            }
            ListKey::Down => {
                selected = (selected + 1) % total_selectable;
            }
            ListKey::Enter => {
                if selected == 0 {
                    return HOST_PEER_ID; // Host is holder
                } else if selected <= peers.len() {
                    return peers[selected - 1]; // A joiner is holder
                } else {
                    // Settings
                    menu::run_settings_inline(app_config, &mut reader).await;
                }
            }
            // Host can't cancel holder selection — stay in the picker.
            ListKey::Cancel | ListKey::Unhandled => {}
        }
    }
}

// ─── Post-game menu ──────────────────────────────────────────────────

enum PostGameAction {
    PlayAgain,
    PickNextHolder,
    Quit,
}

async fn run_post_game_menu(
    conn: &mut NetConnection,
    peers: &mut Vec<PeerId>,
    summary: &game::GameSummary,
) -> io::Result<PostGameAction> {
    let term_size = render::terminal_size();
    render::render_game_summary(
        summary.score,
        summary.total_questions,
        &summary.missed_words,
        summary.game_time,
        summary.all_used,
        &[
            "[P] Play again (same holder)",
            "[N] Pick next holder",
            "[Q] Quit session",
        ],
        term_size,
    );

    let mut reader = EventStream::new();
    loop {
        tokio::select! {
            event = reader.next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if crate::input::is_ctrl_c(&key) {
                        crate::render::force_exit();
                    }
                    // Esc / q / Q all take the quit path via classify_key's
                    // Cancel; P/N are hotkeys that fall through Unhandled.
                    if let ListKey::Cancel = list_menu::classify_key(&key) {
                        conn.send_client_msg(&ClientMessage::GameData {
                            msg: GameMessage::QuitSession,
                            target: None,
                        }).await?;
                        return Ok(PostGameAction::Quit);
                    }
                    match key.code {
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            conn.send_client_msg(&ClientMessage::GameData {
                                msg: GameMessage::PlayAgain,
                                target: None,
                            }).await?;
                            return Ok(PostGameAction::PlayAgain);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            conn.send_client_msg(&ClientMessage::GameData {
                                msg: GameMessage::PickNextHolder,
                                target: None,
                            }).await?;
                            return Ok(PostGameAction::PickNextHolder);
                        }
                        _ => {}
                    }
                }
            }
            msg = conn.recv_relay_msg() => {
                match msg? {
                    Some(RelayMessage::GameData { msg: GameMessage::QuitSession, .. }) => {
                        return Ok(PostGameAction::Quit);
                    }
                    Some(RelayMessage::PeerDisconnected { peer_id }) => {
                        peers.retain(|&p| p != peer_id);
                        if peers.is_empty() {
                            return Ok(PostGameAction::Quit);
                        }
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

/// Render the end-of-game summary with the supplied action hints, then block
/// until the user presses any key. Used when there's no interactive post-game
/// menu to run (solo fallback paths like a lost relay connection).
async fn show_summary_until_keypress(summary: &game::GameSummary, actions: &[&str]) {
    let term_size = render::terminal_size();
    render::render_game_summary(
        summary.score,
        summary.total_questions,
        &summary.missed_words,
        summary.game_time,
        summary.all_used,
        actions,
        term_size,
    );

    let mut reader = EventStream::new();
    while let Some(Ok(event)) = reader.next().await {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if crate::input::is_ctrl_c(&key) {
                crate::render::force_exit();
            }
            break;
        }
    }
}
