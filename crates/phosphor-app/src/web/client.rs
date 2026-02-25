use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use tungstenite::protocol::Message;
use tungstenite::WebSocket;

use super::types::WsInMessage;
use crate::midi::types::TriggerAction;

/// Run the per-client read/write loop.
/// Reads JSON from the client, sends outbound messages from the broadcast channel.
pub fn run_client<S: Read + Write>(
    mut ws: WebSocket<S>,
    inbound_tx: Sender<WsInMessage>,
    outbound_rx: Receiver<String>,
    initial_state: String,
    shutdown: Arc<AtomicBool>,
    client_id: usize,
) {
    log::info!("WebSocket client {} connected", client_id);

    // Send initial full state
    if !initial_state.is_empty() {
        if ws.send(Message::text(initial_state)).is_err() {
            log::info!("WebSocket client {} disconnected on initial send", client_id);
            return;
        }
    }

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // Try to read a message (50ms timeout)
        match ws.read() {
            Ok(Message::Text(text)) => {
                if let Some(msg) = parse_client_message(text.as_ref()) {
                    let _ = inbound_tx.try_send(msg);
                }
            }
            Ok(Message::Close(_)) => {
                log::info!("WebSocket client {} closed connection", client_id);
                break;
            }
            Ok(Message::Ping(data)) => {
                let _ = ws.send(Message::Pong(data));
            }
            Ok(_) => {} // Binary, Pong, etc.
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout — expected, continue to drain outbound
            }
            Err(e) => {
                log::debug!("WebSocket client {} read error: {e}", client_id);
                break;
            }
        }

        // Drain outbound messages
        let mut sent_any = false;
        for msg in outbound_rx.try_iter() {
            if msg.is_empty() { continue; }
            match ws.send(Message::text(msg)) {
                Ok(_) => sent_any = true,
                Err(e) => {
                    log::debug!("WebSocket client {} write error: {e}", client_id);
                    return;
                }
            }
        }

        // Flush if we sent anything
        if sent_any {
            if ws.flush().is_err() {
                break;
            }
        }
    }

    let _ = ws.close(None);
    log::info!("WebSocket client {} disconnected", client_id);
}

/// Parse a JSON message from the client into a WsInMessage.
fn parse_client_message(text: &str) -> Option<WsInMessage> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let msg_type = v.get("type")?.as_str()?;

    match msg_type {
        "set_param" => {
            let name = v.get("name")?.as_str()?.to_string();
            let value = v.get("value")?.as_f64()? as f32;
            Some(WsInMessage::SetParam { name, value })
        }
        "set_layer_param" => {
            let layer = v.get("layer")?.as_u64()? as usize;
            let name = v.get("name")?.as_str()?.to_string();
            let value = v.get("value")?.as_f64()? as f32;
            Some(WsInMessage::SetLayerParam { layer, name, value })
        }
        "load_effect" => {
            let index = v.get("index")?.as_u64()? as usize;
            Some(WsInMessage::LoadEffect { index })
        }
        "select_layer" => {
            let index = v.get("index")?.as_u64()? as usize;
            Some(WsInMessage::SelectLayer { index })
        }
        "set_layer_opacity" => {
            let layer = v.get("layer")?.as_u64()? as usize;
            let value = v.get("value")?.as_f64()? as f32;
            Some(WsInMessage::SetLayerOpacity {
                layer,
                value: value.clamp(0.0, 1.0),
            })
        }
        "set_layer_blend" => {
            let layer = v.get("layer")?.as_u64()? as usize;
            let value = v.get("value")?.as_u64()? as u32;
            Some(WsInMessage::SetLayerBlend { layer, value })
        }
        "set_layer_enabled" => {
            let layer = v.get("layer")?.as_u64()? as usize;
            let value = v.get("value")?.as_bool().or_else(|| {
                v.get("value")?.as_f64().map(|f| f > 0.5)
            })?;
            Some(WsInMessage::SetLayerEnabled { layer, value })
        }
        "trigger" => {
            let action_str = v.get("action")?.as_str()?;
            let action = match action_str {
                "next_effect" => TriggerAction::NextEffect,
                "prev_effect" => TriggerAction::PrevEffect,
                "toggle_postprocess" => TriggerAction::TogglePostProcess,
                "toggle_overlay" => TriggerAction::ToggleOverlay,
                "next_preset" => TriggerAction::NextPreset,
                "prev_preset" => TriggerAction::PrevPreset,
                "next_layer" => TriggerAction::NextLayer,
                "prev_layer" => TriggerAction::PrevLayer,
                _ => return None,
            };
            Some(WsInMessage::Trigger(action))
        }
        "load_preset" => {
            let index = v.get("index")?.as_u64()? as usize;
            Some(WsInMessage::LoadPreset { index })
        }
        "set_postprocess_enabled" => {
            let value = v.get("value")?.as_bool().or_else(|| {
                v.get("value")?.as_f64().map(|f| f > 0.5)
            })?;
            Some(WsInMessage::PostProcessEnabled(value))
        }
        _ => {
            log::debug!("Unknown WS message type: {msg_type}");
            None
        }
    }
}
