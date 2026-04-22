mod room_codes;

use clap::Parser;
use protocol::{
    read_frame, write_frame, ClientMessage, PeerId, RelayError, RelayMessage, HOST_PEER_ID,
};
use room_codes::{MAX_POOL_ATTEMPTS, POOL};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info, warn};

const MAX_PEERS_PER_ROOM: usize = 8;

#[derive(Parser, Debug)]
#[command(version, about = "Guess Up relay server")]
struct Args {
    /// Address to bind to
    #[arg(long, default_value = "0.0.0.0:3000")]
    bind: String,

    /// Maximum number of rooms
    #[arg(long, default_value_t = 100)]
    max_rooms: usize,

    /// Room timeout in seconds
    #[arg(long, default_value_t = 3600)]
    room_timeout: u64,
}

struct Peer {
    tx: mpsc::Sender<RelayMessage>,
    peer_id: PeerId,
}

struct Room {
    host_tx: mpsc::Sender<RelayMessage>,
    peers: Vec<Peer>,
    next_peer_id: PeerId,
    created_at: Instant,
}

impl Room {
    /// Send a message to all peers, optionally excluding one.
    async fn broadcast_to_peers(&self, msg: &RelayMessage, exclude: Option<PeerId>) {
        for peer in &self.peers {
            if exclude == Some(peer.peer_id) {
                continue;
            }
            let _ = peer.tx.send(msg.clone()).await;
        }
    }

    /// Send a message to a specific peer. Returns false if peer not found.
    async fn send_to_peer(&self, peer_id: PeerId, msg: RelayMessage) -> bool {
        for peer in &self.peers {
            if peer.peer_id == peer_id {
                let _ = peer.tx.send(msg).await;
                return true;
            }
        }
        false
    }

    /// Remove a peer from the room by ID.
    fn remove_peer(&mut self, peer_id: PeerId) {
        self.peers.retain(|p| p.peer_id != peer_id);
    }

    /// Get a list of all peer IDs currently in the room.
    fn peer_ids(&self) -> Vec<PeerId> {
        self.peers.iter().map(|p| p.peer_id).collect()
    }
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

    fn generate_random_code() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..5).map(|_| rng.gen_range(b'A'..=b'Z') as char).collect()
    }

    /// Pick a fresh code: try the ASOIAF pool up to MAX_POOL_ATTEMPTS times
    /// before falling back to the random A-Z generator.
    fn pick_code(rooms: &HashMap<String, Arc<Mutex<Room>>>) -> String {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        for _ in 0..MAX_POOL_ATTEMPTS {
            if let Some(candidate) = POOL.choose(&mut rng) {
                if !rooms.contains_key(*candidate) {
                    return (*candidate).to_string();
                }
            }
        }
        loop {
            let candidate = Self::generate_random_code();
            if !rooms.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    async fn create_room(
        &self,
        host_tx: mpsc::Sender<RelayMessage>,
    ) -> Result<(String, Arc<Mutex<Room>>), RelayError> {
        let rooms = self.rooms.read().await;
        if rooms.len() >= self.max_rooms {
            return Err(RelayError::ServerFull);
        }
        drop(rooms);

        let mut rooms = self.rooms.write().await;
        let code = Self::pick_code(&rooms);

        let room = Arc::new(Mutex::new(Room {
            host_tx,
            peers: Vec::new(),
            next_peer_id: 1,
            created_at: Instant::now(),
        }));

        rooms.insert(code.clone(), Arc::clone(&room));

        Ok((code, room))
    }

    async fn join_room(
        &self,
        code: &str,
        joiner_tx: mpsc::Sender<RelayMessage>,
    ) -> Result<(Arc<Mutex<Room>>, PeerId), RelayError> {
        let rooms = self.rooms.read().await;
        let room = rooms.get(code).ok_or(RelayError::RoomNotFound)?;
        let room = Arc::clone(room);
        drop(rooms);

        let mut room_guard = room.lock().await;
        if room_guard.peers.len() >= MAX_PEERS_PER_ROOM {
            return Err(RelayError::RoomFull);
        }

        let peer_id = room_guard.next_peer_id;
        room_guard.next_peer_id = room_guard.next_peer_id.wrapping_add(1);

        // Notify host and all existing peers
        let join_msg = RelayMessage::PeerJoined { peer_id };
        let _ = room_guard.host_tx.send(join_msg.clone()).await;
        room_guard.broadcast_to_peers(&join_msg, None).await;

        room_guard.peers.push(Peer {
            tx: joiner_tx,
            peer_id,
        });

        drop(room_guard);
        Ok((room, peer_id))
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
        ClientMessage::JoinRoom { code } => {
            let code = code.to_ascii_uppercase();
            handle_joiner(server, &code, reader, writer).await
        }
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

    let (code, room) = match server.create_room(tx).await {
        Ok(result) => result,
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

    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.tick().await; // skip first immediate tick

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(relay_msg) => {
                        write_frame(&mut writer, &relay_msg).await?;
                    }
                    None => break,
                }
            }
            msg = read_frame::<ClientMessage, _>(&mut reader) => {
                match msg? {
                    Some(ClientMessage::GameData { msg: game_msg, target: None }) => {
                        // Broadcast to all peers
                        let relay_msg = RelayMessage::GameData { msg: game_msg, from: HOST_PEER_ID };
                        let room_guard = room.lock().await;
                        room_guard.broadcast_to_peers(&relay_msg, None).await;
                    }
                    Some(ClientMessage::GameData { msg: game_msg, target: Some(id) }) => {
                        // Send to specific peer
                        let relay_msg = RelayMessage::GameData { msg: game_msg, from: HOST_PEER_ID };
                        let room_guard = room.lock().await;
                        room_guard.send_to_peer(id, relay_msg).await;
                    }
                    Some(ClientMessage::Disconnect) | None => {
                        // Notify all peers that host disconnected
                        let room_guard = room.lock().await;
                        let disconnect_msg = RelayMessage::PeerDisconnected { peer_id: HOST_PEER_ID };
                        room_guard.broadcast_to_peers(&disconnect_msg, None).await;
                        drop(room_guard);
                        break;
                    }
                    Some(ClientMessage::Pong) => {}
                    Some(_) => {} // Ignore unexpected messages
                }
            }
            _ = ping_interval.tick() => {
                if write_frame(&mut writer, &RelayMessage::Ping).await.is_err() {
                    let room_guard = room.lock().await;
                    let disconnect_msg = RelayMessage::PeerDisconnected { peer_id: HOST_PEER_ID };
                    room_guard.broadcast_to_peers(&disconnect_msg, None).await;
                    drop(room_guard);
                    break;
                }
            }
        }
    }

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

    let (room, peer_id) = match server.join_room(code, tx).await {
        Ok(result) => result,
        Err(e) => {
            write_frame(&mut writer, &RelayMessage::Error(e)).await?;
            return Ok(());
        }
    };

    info!("Peer {} connected to room {}", peer_id, code);

    // Tell the joiner their ID and who's already in the room
    write_frame(&mut writer, &RelayMessage::JoinedRoom { peer_id }).await?;
    let existing_peers = {
        let room_guard = room.lock().await;
        room_guard.peer_ids()
    };
    write_frame(
        &mut writer,
        &RelayMessage::PeerList {
            peers: existing_peers,
        },
    )
    .await?;

    let host_tx = {
        let room_guard = room.lock().await;
        room_guard.host_tx.clone()
    };

    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.tick().await; // skip first immediate tick

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(relay_msg) => {
                        write_frame(&mut writer, &relay_msg).await?;
                    }
                    None => break,
                }
            }
            msg = read_frame::<ClientMessage, _>(&mut reader) => {
                match msg? {
                    Some(ClientMessage::GameData { msg: game_msg, .. }) => {
                        // Joiner always sends to host
                        let relay_msg = RelayMessage::GameData { msg: game_msg, from: peer_id };
                        let _ = host_tx.send(relay_msg).await;
                    }
                    Some(ClientMessage::Disconnect) | None => {
                        // Notify host and remaining peers
                        let disconnect_msg = RelayMessage::PeerDisconnected { peer_id };
                        let _ = host_tx.send(disconnect_msg.clone()).await;
                        let mut room_guard = room.lock().await;
                        room_guard.remove_peer(peer_id);
                        room_guard.broadcast_to_peers(&disconnect_msg, None).await;
                        drop(room_guard);
                        break;
                    }
                    Some(ClientMessage::Pong) => {}
                    Some(_) => {} // Ignore unexpected messages
                }
            }
            _ = ping_interval.tick() => {
                if write_frame(&mut writer, &RelayMessage::Ping).await.is_err() {
                    let disconnect_msg = RelayMessage::PeerDisconnected { peer_id };
                    let _ = host_tx.send(disconnect_msg.clone()).await;
                    let mut room_guard = room.lock().await;
                    room_guard.remove_peer(peer_id);
                    room_guard.broadcast_to_peers(&disconnect_msg, None).await;
                    drop(room_guard);
                    break;
                }
            }
        }
    }

    info!("Peer {} disconnected from room {}", peer_id, code);
    Ok(())
}
