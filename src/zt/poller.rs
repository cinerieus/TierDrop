use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, Notify, RwLock};
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{debug, warn};

use super::client::ZtClient;
use super::models::{ControllerMember, ControllerNetwork, ZtState};
use crate::sse::SseEvent;

pub async fn start_poller(
    client: ZtClient,
    state: Arc<RwLock<ZtState>>,
    tx: broadcast::Sender<SseEvent>,
    notify: Arc<Notify>,
    poll_interval: Duration,
) {
    let mut tick = interval(poll_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = notify.notified() => {
                debug!("Immediate poll triggered by handler");
            }
        }

        let new_state = poll_once(&client).await;

        // Read old state and compare
        let (status_changed, error_changed, ctrl_networks_changed, ctrl_members_changed) = {
            let old = state.read().await;
            (
                new_state.status != old.status,
                new_state.error != old.error,
                new_state.controller_networks != old.controller_networks,
                new_state.controller_members != old.controller_members,
            )
        };

        // Write new state (brief lock)
        {
            let mut w = state.write().await;
            *w = new_state;
        }

        // Broadcast change events outside of any lock
        if status_changed || error_changed {
            debug!("Status changed, broadcasting SSE event");
            let _ = tx.send(SseEvent::StatusChanged);
        }
        if ctrl_networks_changed {
            debug!("Controller networks changed, broadcasting SSE event");
            let _ = tx.send(SseEvent::ControllerNetworksChanged);
        }
        if ctrl_members_changed {
            debug!("Controller members changed, broadcasting SSE event");
            let _ = tx.send(SseEvent::ControllerMembersChanged);
        }
    }
}

async fn poll_once(client: &ZtClient) -> ZtState {
    // Phase 1: Fetch node status and controller network IDs concurrently
    let (status_res, ctrl_nw_ids_res) = tokio::join!(
        client.get_status(),
        client.get_controller_networks(),
    );

    let mut error = None;

    let status = match status_res {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("Failed to poll ZT status: {}", e);
            error = Some(e);
            None
        }
    };

    let ctrl_nw_ids = match ctrl_nw_ids_res {
        Ok(ids) => ids,
        Err(e) => {
            debug!("Controller not available: {}", e);
            vec![]
        }
    };

    // Phase 2: Spawn a task per network for true parallelism across threads
    let mut controller_networks: Vec<ControllerNetwork> = Vec::new();
    let mut controller_members: HashMap<String, Vec<ControllerMember>> = HashMap::new();

    if !ctrl_nw_ids.is_empty() {
        let handles: Vec<_> = ctrl_nw_ids
            .into_iter()
            .map(|nwid| {
                let client = client.clone();
                tokio::spawn(async move { fetch_network(&client, &nwid).await })
            })
            .collect();

        for handle in handles {
            if let Ok((nwid, nw_result, members)) = handle.await {
                if let Ok(nw) = nw_result {
                    controller_networks.push(nw);
                }
                controller_members.insert(nwid, members);
            }
        }
    }

    ZtState {
        status,
        controller_networks,
        controller_members,
        last_updated: Some(SystemTime::now()),
        error,
    }
}

/// Fetch a single network's details and all its members concurrently.
async fn fetch_network(
    client: &ZtClient,
    nwid: &str,
) -> (
    String,
    Result<ControllerNetwork, String>,
    Vec<ControllerMember>,
) {
    // Fetch network detail and member ID list in parallel
    let (nw_result, member_ids_result) = tokio::join!(
        client.get_controller_network(nwid),
        client.get_controller_members(nwid),
    );

    let members = match member_ids_result {
        Ok(ids) => {
            // Fetch all member details in parallel via spawned tasks
            let handles: Vec<_> = ids
                .keys()
                .map(|mid| {
                    let client = client.clone();
                    let nwid = nwid.to_string();
                    let mid = mid.clone();
                    tokio::spawn(async move { client.get_controller_member(&nwid, &mid).await })
                })
                .collect();

            let mut members = Vec::with_capacity(handles.len());
            for handle in handles {
                if let Ok(Ok(m)) = handle.await {
                    members.push(m);
                }
            }
            // Sort by ID for stable PartialEq comparison between polls
            members.sort_by(|a, b| a.display_id().cmp(b.display_id()));
            members
        }
        Err(_) => vec![],
    };

    (nwid.to_string(), nw_result, members)
}
