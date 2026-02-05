use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

#[derive(Debug, Clone)]
pub enum SseEvent {
    StatusChanged,
    ControllerNetworksChanged,
    ControllerMembersChanged,
}

impl SseEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            SseEvent::StatusChanged => "status-changed",
            SseEvent::ControllerNetworksChanged => "ctrl-networks-changed",
            SseEvent::ControllerMembersChanged => "ctrl-members-changed",
        }
    }
}

pub async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let sse_event = Event::default()
                .event(event.event_name())
                .data("");
            Some(Ok(sse_event))
        }
        Err(_) => None, // Lagged — skip, next poll cycle will catch up
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
