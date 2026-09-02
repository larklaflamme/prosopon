//! HTTP signaling endpoint (Option B — Lark's decision, 2026-09-02).
//!
//! WebRTC requires an SDP offer/answer exchange before the data channel opens.
//! This module exposes a tiny HTTP endpoint that receives the client's offer
//! and returns the server's answer.
//!
//! The client POSTs a JSON `RTCSessionDescription`:
//!
//! ```json
//! { "type": "offer", "sdp": "v=0\r\n..." }
//! ```
//!
//! and receives the answer in the same shape:
//!
//! ```json
//! { "type": "answer", "sdp": "v=0\r\n..." }
//! ```
//!
//! M0 runs this over plain HTTP on localhost (or an SSH tunnel); HTTPS is a
//! later version (Lark: "we will switch to HTTPS later").

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use std::sync::Arc;
use webrtc::peer_connection::RTCSessionDescription;

use crate::webrtc::WebRtcServer;

/// Shared state for the signaling endpoint: the WebRTC server that answers
/// offers.
pub struct SignalingState {
    server: Arc<WebRtcServer>,
}

/// Build the signaling router: `POST /offer` → answer.
pub fn router(server: Arc<WebRtcServer>) -> Router {
    Router::new()
        .route("/offer", post(handle_offer))
        .with_state(Arc::new(SignalingState { server }))
}

/// Handle an offer: set it as the remote description, create the answer, and
/// return it to the client.
async fn handle_offer(
    State(state): State<Arc<SignalingState>>,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, (StatusCode, String)> {
    state
        .server
        .answer(offer)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::pipeline::Pipeline;

    #[tokio::test]
    async fn router_builds_with_default_config() {
        let mut config = Config::default();
        // Bind to an ephemeral port so this test doesn't collide with the
        // webrtc module's own test (both would otherwise grab 29434).
        config.webrtc.listen_port = 0;
        let pipeline = Arc::new(Pipeline::new(&config));
        let server = Arc::new(
            WebRtcServer::new(&config.webrtc, pipeline)
                .await
                .expect("server should build with host-only ICE"),
        );
        // The router builds and holds the server in shared state.
        let _app = router(server);
    }
}
