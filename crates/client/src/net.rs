use crate::types::*;
use protocol::{read_frame, write_frame, ClientMessage, GameMessage, PeerId, RelayMessage};
use tokio::io::{BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

pub type Reader = BufReader<OwnedReadHalf>;
pub type Writer = BufWriter<OwnedWriteHalf>;

/// Outbound message from the game loop to the net write task.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum OutboundMsg {
    Broadcast(GameMessage),
    SendTo(PeerId, GameMessage),
}

pub struct NetConnection {
    pub reader: Reader,
    pub writer: Writer,
}

impl NetConnection {
    pub async fn connect(addr: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
        })
    }

    pub async fn send_client_msg(&mut self, msg: &ClientMessage) -> std::io::Result<()> {
        write_frame(&mut self.writer, msg).await
    }

    pub async fn recv_relay_msg(&mut self) -> std::io::Result<Option<RelayMessage>> {
        read_frame(&mut self.reader).await
    }

    /// Split into reader/writer for use by net tasks, reassemble later.
    pub fn into_parts(self) -> (Reader, Writer) {
        (self.reader, self.writer)
    }

    pub fn from_parts(reader: Reader, writer: Writer) -> Self {
        Self { reader, writer }
    }
}

/// Result of spawning net tasks — includes handles and a way to get the connection back.
pub struct NetHandle {
    pub outbound_tx: mpsc::Sender<OutboundMsg>,
    read_handle: tokio::task::JoinHandle<Reader>,
    write_handle: tokio::task::JoinHandle<Writer>,
    stop_tx: oneshot::Sender<()>,
}

impl NetHandle {
    /// Stop the net tasks and recover the underlying connection.
    /// The read task stops when it receives the stop signal.
    /// The write task stops when outbound_tx is dropped (channels closed).
    pub async fn shutdown(self) -> Option<NetConnection> {
        let _ = self.stop_tx.send(());
        drop(self.outbound_tx);

        let reader = self.read_handle.await.ok()?;
        let writer = self.write_handle.await.ok()?;
        Some(NetConnection::from_parts(reader, writer))
    }
}

/// Spawn background tasks that bridge the TCP connection to game event channels.
/// Returns a NetHandle that can be used to send messages and recover the connection.
pub fn spawn_net_tasks(conn: NetConnection, event_tx: EventSender) -> NetHandle {
    let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundMsg>(32);
    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    let (reader, writer) = conn.into_parts();

    let read_handle = tokio::spawn(net_read_task(reader, event_tx, stop_rx));
    let write_handle = tokio::spawn(net_write_task(writer, outbound_rx));

    NetHandle {
        outbound_tx,
        read_handle,
        write_handle,
        stop_tx,
    }
}

/// Reads RelayMessages from the TCP stream and translates them into GameEvents.
/// Returns the reader when stopped so the connection can be reused.
async fn net_read_task(
    mut reader: Reader,
    event_tx: EventSender,
    mut stop_rx: oneshot::Receiver<()>,
) -> Reader {
    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            result = read_frame::<RelayMessage, _>(&mut reader) => {
                match result {
                    Ok(Some(msg)) => {
                        let event = match msg {
                            RelayMessage::GameData { msg: game_msg, from } => {
                                translate_game_message(game_msg, from)
                            }
                            RelayMessage::PeerDisconnected { peer_id } => {
                                Some(GameEvent::PeerDisconnected(peer_id))
                            }
                            RelayMessage::PeerJoined { peer_id } => {
                                Some(GameEvent::PeerJoined(peer_id))
                            }
                            RelayMessage::PeerList { peers } => {
                                Some(GameEvent::PeerList(peers))
                            }
                            RelayMessage::Ping => None, // Pong handled at lobby level
                            _ => None,
                        };
                        if let Some(ev) = event {
                            if event_tx.send(ev).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        let _ = event_tx.send(GameEvent::PeerDisconnected(0)).await;
                        break;
                    }
                    Err(_) => {
                        let _ = event_tx.send(GameEvent::PeerDisconnected(0)).await;
                        break;
                    }
                }
            }
        }
    }
    reader
}

fn translate_game_message(msg: GameMessage, from: PeerId) -> Option<GameEvent> {
    match msg {
        GameMessage::PlayerInput(action) => Some(GameEvent::RemoteInput(from, action.into())),
        GameMessage::WordUpdate { word } => Some(GameEvent::NetWordUpdate(word)),
        GameMessage::TimerSync { seconds_left } => Some(GameEvent::NetTimerSync(seconds_left)),
        GameMessage::ScoreUpdate { score, total } => {
            Some(GameEvent::NetScoreUpdate { score, total })
        }
        GameMessage::Flash(kind) => Some(GameEvent::NetFlash(kind)),
        GameMessage::TimerExpired => Some(GameEvent::NetTimerExpired),
        GameMessage::GameOver(result) => Some(GameEvent::NetGameOver(result)),
        _ => None,
    }
}

/// Reads OutboundMsgs from the channel and writes them to the TCP stream.
/// Returns the writer when the channel closes so the connection can be reused.
async fn net_write_task(
    mut writer: Writer,
    mut outbound_rx: mpsc::Receiver<OutboundMsg>,
) -> Writer {
    while let Some(msg) = outbound_rx.recv().await {
        let client_msg = match msg {
            OutboundMsg::Broadcast(game_msg) => ClientMessage::GameData {
                msg: game_msg,
                target: None,
            },
            OutboundMsg::SendTo(peer_id, game_msg) => ClientMessage::GameData {
                msg: game_msg,
                target: Some(peer_id),
            },
        };
        if write_frame(&mut writer, &client_msg).await.is_err() {
            break;
        }
    }
    writer
}
