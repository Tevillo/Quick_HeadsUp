use crate::types::{BonusReceiver, EventSender, GameEvent};
use std::time::Duration;
use tokio::time::{self, Instant};

/// Seconds remaining at which the low-time warning kicks in.
pub const WARNING_THRESHOLD_SECS: u64 = 10;

/// Blink period for the low-time warning. 500ms = 2Hz (on/off every half-second).
const BLINK_PERIOD: Duration = Duration::from_millis(500);

/// Background task that emits a `BlinkTick` every 500ms for the full duration
/// of the game. The game loop decides when to react (only when
/// `seconds_left <= WARNING_THRESHOLD_SECS`).
pub async fn blink_task(tx: EventSender) {
    let mut interval = time::interval(BLINK_PERIOD);
    interval.tick().await; // Skip the first immediate tick
    loop {
        interval.tick().await;
        if tx.send(GameEvent::BlinkTick).await.is_err() {
            break;
        }
    }
}

pub async fn timer_task(tx: EventSender, duration: u64, mut bonus_rx: BonusReceiver) {
    let mut deadline = Instant::now() + Duration::from_secs(duration);
    let mut interval = time::interval(Duration::from_secs(1));
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Apply any pending bonus time before computing remaining
                while let Ok(bonus) = bonus_rx.try_recv() {
                    deadline += Duration::from_secs(bonus);
                }
                let now = Instant::now();
                if now >= deadline {
                    let _ = tx.send(GameEvent::TimerExpired).await;
                    break;
                }
                let remaining = (deadline - now).as_secs_f64().ceil() as u64;
                let _ = tx.send(GameEvent::TimerTick(remaining)).await;
            }
            Some(bonus) = bonus_rx.recv() => {
                deadline += Duration::from_secs(bonus);
                // Immediately display updated time so the full bonus is visible
                let now = Instant::now();
                if now < deadline {
                    let remaining = (deadline - now).as_secs_f64().ceil() as u64;
                    let _ = tx.send(GameEvent::TimerTick(remaining)).await;
                }
            }
        }
    }
}
