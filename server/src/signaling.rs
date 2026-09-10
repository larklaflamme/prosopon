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
use std::sync::{Arc, Mutex};
use webrtc::peer_connection::{RTCIceCandidateInit, RTCSessionDescription};

use crate::config::WebrtcConfig;
use crate::pipeline::Pipeline;
use crate::webrtc::WebRtcServer;

/// The signaling wire message: an SDP description plus the trickled ICE
/// candidates gathered alongside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingMessage {
    #[serde(flatten)]
    pub description: RTCSessionDescription,
    pub candidates: Vec<RTCIceCandidateInit>,
}

/// Shared state for the signaling endpoint: the WebRTC config and pipeline
/// used to build a *fresh* peer connection per offer, the auth token, and a
/// registry of live sessions.
///
/// A WebRTC peer connection is bound to a single remote peer. Reusing one
/// `pc` across offers leaves it stuck on the first client (the second offer's
/// data channel never opens). So each offer gets its own `WebRtcServer`, and
/// the answered connection is retained in `sessions` so its data channel
/// stays alive for the duration of that client's session.
pub struct SignalingState {
    webrtc_config: WebrtcConfig,
    pipeline: Arc<Pipeline>,
    auth_token: String,
    sessions: Mutex<Vec<Arc<WebRtcServer>>>,
}

/// Build the signaling router: `POST /offer` → answer.
pub fn router(
    webrtc_config: WebrtcConfig,
    pipeline: Arc<Pipeline>,
    auth_token: String,
) -> Router {
    Router::new()
        .route("/offer", post(handle_offer))
        .with_state(Arc::new(SignalingState {
            webrtc_config,
            pipeline,
            auth_token,
            sessions: Mutex::new(Vec::new()),
        }))
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

/// Handle an offer: build a fresh peer connection, set the offer as its remote
/// description, add the client's candidates, create the answer, and return it
/// with the server's candidates. The answered connection is retained so its
/// data channel stays alive.
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

    // A fresh peer connection per offer — a `pc` is single-peer.
    let server = Arc::new(
        WebRtcServer::new(&state.webrtc_config, state.pipeline.clone())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    );
    let answer = server
        .answer(msg.description, msg.candidates)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Retain the connection so its data channel stays alive for the session.
    state.sessions.lock().unwrap().push(server);

    Ok(Json(answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::pipeline::Pipeline;

    #[tokio::test]
    async fn router_builds_with_default_config() {
        let config = Config::default();
        let pipeline = Arc::new(Pipeline::new(&config));
        let _app = router(config.webrtc.clone(), pipeline, String::new());
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
