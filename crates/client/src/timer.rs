use crate::types::{BonusReceiver, EventSender, GameEvent};
use std::time::Duration;
use tokio::time::{self, Instant};

pub async fn timer_task(tx: EventSender, duration: u64, mut bonus_rx: BonusReceiver) {
    let mut deadline = Instant::now() + Duration::from_secs(duration);
    let mut interval = time::interval(Duration::from_secs(1));
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Instant::now();
                if now >= deadline {
                    let _ = tx.send(GameEvent::TimerExpired).await;
                    break;
                }
                let remaining = (deadline - now).as_secs();
                let _ = tx.send(GameEvent::TimerTick(remaining)).await;
            }
            Some(bonus) = bonus_rx.recv() => {
                deadline += Duration::from_secs(bonus);
            }
        }
    }
}
