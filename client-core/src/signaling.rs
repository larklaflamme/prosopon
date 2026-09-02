//! HTTP signaling client (Option B — Lark's decision, 2026-09-02).
//!
//! The client POSTs its SDP offer *plus its trickled ICE candidates* to the
//! server's `/offer` endpoint and receives the server's answer plus the
//! server's candidates. The wire shape is the W3C `RTCSessionDescription`
//! (`{"type": "offer", "sdp": "..."}`) flattened together with a
//! `candidates` array of `RTCIceCandidateInit`.

use serde::{Deserialize, Serialize};
use webrtc::peer_connection::{RTCIceCandidateInit, RTCSessionDescription};

use crate::ClientError;

/// The signaling wire message: an SDP description plus the trickled ICE
/// candidates gathered alongside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingMessage {
    #[serde(flatten)]
    pub description: RTCSessionDescription,
    pub candidates: Vec<RTCIceCandidateInit>,
}

/// HTTP client for the SDP + ICE candidate exchange.
pub struct SignalingClient {
    url: String,
    auth_token: String,
    http: reqwest::Client,
}

impl SignalingClient {
    /// Build a signaling client targeting `url` (the server's `/offer`),
    /// presenting `auth_token` as `Authorization: Bearer <token>`.
    pub fn new(url: &str, auth_token: &str) -> Self {
        Self {
            url: url.to_string(),
            auth_token: auth_token.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Send an offer (with candidates) and return the server's answer (with
    /// candidates).
    pub async fn offer(
        &self,
        msg: SignalingMessage,
    ) -> Result<SignalingMessage, ClientError> {
        let mut req = self.http.post(&self.url).json(&msg);
        if !self.auth_token.is_empty() {
            req = req.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.auth_token),
            );
        }
        let resp = req.send().await?.error_for_status()?;
        let answer: SignalingMessage = resp.json().await?;
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
