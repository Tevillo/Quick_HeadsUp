use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ─── Constants ───────────────────────────────────────────────────────
const MAX_FRAME_SIZE: u32 = 65_536; // 64KB

/// Magic string sent as the first handshake frame from client to relay.
/// The relay hard-rejects connections that don't send this exact value.
pub const HANDSHAKE_MAGIC: &str = "GUESSUP";

// ─── Peer ID ─────────────────────────────────────────────────────────

pub type PeerId = u8;
pub const HOST_PEER_ID: PeerId = 0;

// ─── Handshake ───────────────────────────────────────────────────────

/// First frame the client sends on connect. The relay validates both the
/// magic and the exact crate version before accepting further messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub magic: String,
    pub version: String,
}

/// Relay's reply to a `Handshake`. Anything other than `Ok` is followed by
/// the relay closing the connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandshakeResponse {
    Ok,
    InvalidMagic,
    VersionMismatch { relay_version: String },
}

// ─── Framing ─────────────────────────────────────────────────────────

/// Write a length-prefixed JSON frame to an async writer.
pub async fn write_frame<T: Serialize, W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    let json = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-prefixed JSON frame from an async reader.
/// Returns `Ok(None)` on clean EOF.
pub async fn read_frame<T: DeserializeOwned, R: AsyncReadExt + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<T>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {} bytes (max {})", len, MAX_FRAME_SIZE),
        ));
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;
    let msg = serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(msg))
}

// ─── Client → Relay ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    CreateRoom,
    JoinRoom {
        code: String,
    },
    GameData {
        msg: GameMessage,
        target: Option<PeerId>,
    },
    Disconnect,
    Pong,
}

// ─── Relay → Client ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelayMessage {
    RoomCreated { code: String },
    PeerJoined { peer_id: PeerId },
    JoinedRoom { peer_id: PeerId },
    PeerList { peers: Vec<PeerId> },
    GameData { msg: GameMessage, from: PeerId },
    PeerDisconnected { peer_id: PeerId },
    Error(RelayError),
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelayError {
    RoomNotFound,
    RoomFull,
    InvalidCode,
    ServerFull,
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::RoomNotFound => write!(f, "room not found"),
            RelayError::RoomFull => write!(f, "room is full"),
            RelayError::InvalidCode => write!(f, "invalid room code"),
            RelayError::ServerFull => write!(f, "server is full"),
        }
    }
}

// ─── Peer ↔ Peer (forwarded through relay) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameMessage {
    // Lobby
    RoleAssignment { holder_id: PeerId },
    RoleAccepted,
    GameStart(NetGameConfig),

    // In-game: host → remote
    WordUpdate { word: String },
    TimerSync { seconds_left: u64 },
    ScoreUpdate { score: usize, total: usize },
    Flash(FlashKind),
    TimerExpired,
    GameOver(NetGameResult),

    // In-game: remote → host
    PlayerInput(NetUserAction),

    // Post-game
    PlayAgain,
    PickNextHolder,
    QuitSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Viewer,
    Holder,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Viewer => write!(f, "Viewer"),
            Role::Holder => write!(f, "Holder"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlashKind {
    Correct,
    Incorrect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetUserAction {
    Correct,
    Pass,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetGameConfig {
    pub game_time: u64,
    pub last_unlimited: bool,
    pub extra_time: bool,
    pub bonus_seconds: u64,
    pub word_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetGameResult {
    pub score: usize,
    pub total_questions: usize,
    pub missed_words: Vec<String>,
    pub game_time: u64,
    pub all_used: bool,
}
