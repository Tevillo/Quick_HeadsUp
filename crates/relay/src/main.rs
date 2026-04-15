use clap::Parser;
use protocol::{read_frame, write_frame, ClientMessage, RelayError, RelayMessage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(version, about = "Heads Up relay server")]
struct Args {
    /// Address to bind to
    #[arg(long, default_value = "0.0.0.0:7878")]
    bind: String,

    /// Maximum number of rooms
    #[arg(long, default_value_t = 100)]
    max_rooms: usize,

    /// Room timeout in seconds
    #[arg(long, default_value_t = 3600)]
    room_timeout: u64,
}

struct Room {
    host_tx: mpsc::Sender<RelayMessage>,
    joiner_tx: Option<mpsc::Sender<RelayMessage>>,
    created_at: Instant,
}

struct RelayServer {
    rooms: RwLock<HashMap<String, Arc<Mutex<Room>>>>,
    max_rooms: usize,
    room_timeout: Duration,
}

impl RelayServer {
    fn new(max_rooms: usize, room_timeout: Duration) -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            max_rooms,
            room_timeout,
        }
    }

    fn generate_code() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..5).map(|_| rng.gen_range(b'A'..=b'Z') as char).collect()
    }

    async fn create_room(&self, host_tx: mpsc::Sender<RelayMessage>) -> Result<String, RelayError> {
        let rooms = self.rooms.read().await;
        if rooms.len() >= self.max_rooms {
            return Err(RelayError::ServerFull);
        }
        drop(rooms);

        let mut rooms = self.rooms.write().await;
        // Generate a unique code
        let code = loop {
            let candidate = Self::generate_code();
            if !rooms.contains_key(&candidate) {
                break candidate;
            }
        };

        rooms.insert(
            code.clone(),
            Arc::new(Mutex::new(Room {
                host_tx,
                joiner_tx: None,
                created_at: Instant::now(),
            })),
        );

        Ok(code)
    }

    async fn join_room(
        &self,
        code: &str,
        joiner_tx: mpsc::Sender<RelayMessage>,
    ) -> Result<Arc<Mutex<Room>>, RelayError> {
        let rooms = self.rooms.read().await;
        let room = rooms.get(code).ok_or(RelayError::RoomNotFound)?;
        let room = Arc::clone(room);
        drop(rooms);

        let mut room_guard = room.lock().await;
        if room_guard.joiner_tx.is_some() {
            return Err(RelayError::RoomFull);
        }
        room_guard.joiner_tx = Some(joiner_tx);

        // Notify host
        let _ = room_guard.host_tx.send(RelayMessage::PeerJoined).await;

        drop(room_guard);
        Ok(room)
    }

    async fn remove_room(&self, code: &str) {
        self.rooms.write().await.remove(code);
    }

    async fn reap_stale_rooms(&self) {
        let mut rooms = self.rooms.write().await;
        let timeout = self.room_timeout;
        rooms.retain(|code, room| {
            // Try to lock non-blockingly; if locked, it's active
            match room.try_lock() {
                Ok(r) => {
                    let keep = r.created_at.elapsed() < timeout;
                    if !keep {
                        info!("Reaping stale room {}", code);
                    }
                    keep
                }
                Err(_) => true,
            }
        });
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let server = Arc::new(RelayServer::new(
        args.max_rooms,
        Duration::from_secs(args.room_timeout),
    ));

    let listener = TcpListener::bind(&args.bind).await?;
    info!("Relay listening on {}", args.bind);

    // Spawn room reaper
    let reaper_server = Arc::clone(&server);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            reaper_server.reap_stale_rooms().await;
        }
    });

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("New connection from {}", addr);
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(server, stream).await {
                warn!("Connection error from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(server: Arc<RelayServer>, stream: TcpStream) -> std::io::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

    // Read the first message to determine role
    let first_msg: ClientMessage = match read_frame(&mut reader).await? {
        Some(msg) => msg,
        None => return Ok(()),
    };

    match first_msg {
        ClientMessage::CreateRoom => handle_host(server, reader, writer).await,
        ClientMessage::JoinRoom { code } => handle_joiner(server, &code, reader, writer).await,
        _ => {
            write_frame(&mut writer, &RelayMessage::Error(RelayError::InvalidCode)).await?;
            Ok(())
        }
    }
}

async fn handle_host(
    server: Arc<RelayServer>,
    mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    mut writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
) -> std::io::Result<()> {
    let (tx, mut rx) = mpsc::channel::<RelayMessage>(32);

    let code = match server.create_room(tx).await {
        Ok(code) => code,
        Err(e) => {
            write_frame(&mut writer, &RelayMessage::Error(e)).await?;
            return Ok(());
        }
    };

    info!("Room {} created", code);
    write_frame(
        &mut writer,
        &RelayMessage::RoomCreated { code: code.clone() },
    )
    .await?;

    // Wait for peer joined or host messages
    // First, wait until a joiner connects by forwarding messages from the relay channel
    let joiner_tx: mpsc::Sender<RelayMessage>;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(relay_msg) => {
                        write_frame(&mut writer, &relay_msg).await?;
                        if matches!(relay_msg, RelayMessage::PeerJoined) {
                            // Get joiner_tx from the room
                            let rooms = server.rooms.read().await;
                            let room = rooms.get(&code).unwrap();
                            let room_guard = room.lock().await;
                            joiner_tx = room_guard.joiner_tx.clone().unwrap();
                            drop(room_guard);
                            drop(rooms);
                            break;
                        }
                    }
                    None => {
                        server.remove_room(&code).await;
                        return Ok(());
                    }
                }
            }
            msg = read_frame::<ClientMessage, _>(&mut reader) => {
                match msg? {
                    Some(ClientMessage::Disconnect) | None => {
                        server.remove_room(&code).await;
                        return Ok(());
                    }
                    Some(ClientMessage::Pong) => {}
                    Some(_) => {} // Ignore other messages before peer joins
                }
            }
        }
    }

    // Both connected — run forwarding
    run_forwarding(
        &code,
        &server,
        &mut reader,
        &mut writer,
        &mut rx,
        &joiner_tx,
        true,
    )
    .await?;

    server.remove_room(&code).await;
    info!("Room {} closed (host disconnected)", code);
    Ok(())
}

async fn handle_joiner(
    server: Arc<RelayServer>,
    code: &str,
    mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    mut writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
) -> std::io::Result<()> {
    let (tx, mut rx) = mpsc::channel::<RelayMessage>(32);

    let room = match server.join_room(code, tx).await {
        Ok(room) => room,
        Err(e) => {
            write_frame(&mut writer, &RelayMessage::Error(e)).await?;
            return Ok(());
        }
    };

    info!("Joiner connected to room {}", code);
    write_frame(&mut writer, &RelayMessage::JoinedRoom).await?;

    // Get host_tx
    let host_tx = {
        let room_guard = room.lock().await;
        room_guard.host_tx.clone()
    };

    run_forwarding(
        code,
        &server,
        &mut reader,
        &mut writer,
        &mut rx,
        &host_tx,
        false,
    )
    .await?;

    server.remove_room(code).await;
    info!("Room {} closed (joiner disconnected)", code);
    Ok(())
}

async fn run_forwarding(
    code: &str,
    _server: &Arc<RelayServer>,
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: &mut BufWriter<tokio::net::tcp::OwnedWriteHalf>,
    rx: &mut mpsc::Receiver<RelayMessage>,
    peer_tx: &mpsc::Sender<RelayMessage>,
    is_host: bool,
) -> std::io::Result<()> {
    let role = if is_host { "host" } else { "joiner" };
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.tick().await; // skip first immediate tick

    loop {
        tokio::select! {
            // Messages from the relay channel (sent by peer)
            msg = rx.recv() => {
                match msg {
                    Some(relay_msg) => {
                        write_frame(writer, &relay_msg).await?;
                    }
                    None => {
                        info!("Room {} relay channel closed for {}", code, role);
                        break;
                    }
                }
            }

            // Messages from this client's TCP stream
            msg = read_frame::<ClientMessage, _>(reader) => {
                match msg? {
                    Some(ClientMessage::GameData(game_msg)) => {
                        // Forward to peer
                        let _ = peer_tx.send(RelayMessage::GameData(game_msg)).await;
                    }
                    Some(ClientMessage::Pong) => {}
                    Some(ClientMessage::Disconnect) | None => {
                        let _ = peer_tx.send(RelayMessage::PeerDisconnected).await;
                        info!("Room {} {} disconnected", code, role);
                        break;
                    }
                    Some(_) => {} // Ignore unexpected messages
                }
            }

            // Heartbeat ping
            _ = ping_interval.tick() => {
                if write_frame(writer, &RelayMessage::Ping).await.is_err() {
                    let _ = peer_tx.send(RelayMessage::PeerDisconnected).await;
                    break;
                }
            }
        }
    }

    Ok(())
}
