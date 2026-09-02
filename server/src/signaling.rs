//! HTTP signaling endpoint (Option B — Lark's decision, 2026-09-02).
//!
//! WebRTC requires an SDP offer/answer exchange *and* an ICE candidate
//! exchange before the data channel opens. Because the `webrtc` crate uses
//! trickle ICE (candidates are gathered asynchronously, not embedded in the
//! SDP), this endpoint carries both in a single round-trip:
//!
//! ```json
//! {
//!   "type": "offer",
//!   "sdp": "v=0\r\n...",
//!   "candidates": [
//!     { "candidate": "candidate:...", "sdpMid": "", "sdpMLineIndex": 0 }
//!   ]
//! }
//! ```
//!
//! and returns the answer in the same shape. M0 runs this over plain HTTP on
//! localhost (or an SSH tunnel); HTTPS is a later version.

use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use webrtc::peer_connection::{RTCIceCandidateInit, RTCSessionDescription};

use crate::webrtc::WebRtcServer;

/// The signaling wire message: an SDP description plus the trickled ICE
/// candidates gathered alongside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingMessage {
    #[serde(flatten)]
    pub description: RTCSessionDescription,
    pub candidates: Vec<RTCIceCandidateInit>,
}

/// Shared state for the signaling endpoint: the WebRTC server that answers
/// offers.
pub struct SignalingState {
    server: Arc<WebRtcServer>,
    auth_token: String,
}

/// Build the signaling router: `POST /offer` → answer.
pub fn router(server: Arc<WebRtcServer>, auth_token: String) -> Router {
    Router::new()
        .route("/offer", post(handle_offer))
        .with_state(Arc::new(SignalingState { server, auth_token }))
}

/// Check the request's `Authorization: Bearer <token>` header against the
/// configured shared secret, in constant time. Empty token = auth disabled
/// (localhost dev).
fn authorized(auth_token: &str, headers: &HeaderMap) -> bool {
    if auth_token.is_empty() {
        return true;
    }
    let expected = format!("Bearer {}", auth_token);
    match headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        Some(provided) => constant_time_eq(provided.as_bytes(), expected.as_bytes()),
        None => false,
    }
}

/// Constant-time byte comparison (length check first, then XOR-accumulate).
/// Prevents a timing side-channel on the token check. The `subtle` crate is
/// the production-grade equivalent; this is dependency-free for M0.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Handle an offer: set it as the remote description, add the client's
/// candidates, create the answer, and return it with the server's candidates.
///
/// Auth is checked *before* the body is parsed, so an unauthenticated request
/// gets an empty 401 regardless of whether its body is valid JSON. (If we let
/// axum's `Json` extractor run first, a malformed body would return a 422 with
/// an error body, leaking the endpoint's existence and expected shape.)
async fn handle_offer(
    State(state): State<Arc<SignalingState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<SignalingMessage>, (StatusCode, String)> {
    if !authorized(&state.auth_token, &headers) {
        // Unauthenticated: drop silently — no body, no useful information.
        return Err((StatusCode::UNAUTHORIZED, String::new()));
    }
    let msg: SignalingMessage = serde_json::from_slice(&body)
        .map_err(|_| (StatusCode::BAD_REQUEST, String::new()))?;
    state
        .server
        .answer(msg.description, msg.candidates)
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
        let _app = router(server, String::new());
    }

    #[test]
    fn constant_time_eq_matches_and_rejects() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secre"));
        assert!(!constant_time_eq(b"secret", b"secreX"));
        assert!(!constant_time_eq(b"", b"x"));
    }

    #[test]
    fn authorized_checks_bearer_token() {
        let mut headers = HeaderMap::new();
        // Empty token = auth disabled.
        assert!(authorized("", &headers));
        // Token set, no header = rejected.
        assert!(!authorized("secret", &headers));
        // Correct token = accepted.
        headers.insert(AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert!(authorized("secret", &headers));
        // Wrong token = rejected.
        headers.insert(AUTHORIZATION, "Bearer wrong".parse().unwrap());
        assert!(!authorized("secret", &headers));
    }

    #[test]
    fn signaling_message_round_trips() {
        // The wire shape must round-trip: {"type":"offer","sdp":"...","candidates":[...]}.
        let json = r#"{"type":"offer","sdp":"v=0\r\n","candidates":[{"candidate":"candidate:abc","sdpMid":"","sdpMLineIndex":0}]}"#;
        let msg: SignalingMessage = serde_json::from_str(json).expect("deserialize");
        assert_eq!(msg.description.sdp, "v=0\r\n");
        assert_eq!(msg.candidates.len(), 1);
        assert_eq!(msg.candidates[0].candidate, "candidate:abc");
        let back = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(back["type"], "offer");
        assert_eq!(back["candidates"][0]["candidate"], "candidate:abc");
    }
}
