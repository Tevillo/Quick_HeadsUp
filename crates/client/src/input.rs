use crate::types::{EventSender, GameEvent, UserAction};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;

pub async fn input_task(tx: EventSender) {
    let mut reader = EventStream::new();

    while let Some(event) = reader.next().await {
        if let Ok(Event::Key(key)) = event {
            // Only handle key press events (not release/repeat)
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let action = match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => Some(UserAction::Correct),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char(' ') => {
                    Some(UserAction::Pass)
                }
                KeyCode::Char('q') | KeyCode::Esc => Some(UserAction::Quit),
                _ => None,
            };
            if let Some(a) = action {
                if tx.send(GameEvent::UserInput(a)).await.is_err() {
                    break;
                }
            }
        }
    }
}
