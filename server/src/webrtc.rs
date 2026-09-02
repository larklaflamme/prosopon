//! WebRTC data-channel server (Slice 5).
//!
//! Under Option B (Lark's decision, 2026-09-02): the server ships Ogg Opus
//! bytes as-is over the WebRTC **data channel** — no audio track, no
//! transcoding, no demuxing. The client plays the Ogg Opus natively.
//!
//! The server accepts one peer. The client creates the data channel; the
//! server receives it via `on_data_channel`, polls it for text messages,
//! runs each through the pipeline (cognition + TTS), and sends the resulting
//! Ogg Opus bytes back over the same channel.
//!
//! ## Audio chunking (2026-09-02)
//!
//! The SCTP data channel's default max message size is 64 KiB (RFC 8841).
//! Kokoro's Ogg Opus output for a typical sentence is ~76 KiB (measured
//! 2026-09-02), which exceeds that limit. We therefore chunk the audio into
//! 16 KiB pieces and reassemble on the client.
//!
//! ## ICE candidate exchange (2026-09-02)
//!
//! The `webrtc` crate uses **trickle ICE**: candidates are gathered
//! asynchronously and delivered via `on_ice_candidate`, *not* embedded in the
//! SDP. The HTTP signaling channel therefore carries the candidates alongside
//! the SDP (a single non-trickle-style round-trip: the client sends its offer
//! + candidates, the server returns its answer + candidates). See
//! `signaling.rs` for the `SignalingMessage` wire shape.
//!
//! NOTE (2026-09-02): the `webrtc` crate was rewritten since the 0.11-era API
//! this project was originally planned against. This module targets the new
//! 0.20.x API. See `design/14-server-implementation-plan.md` §6.

use crate::cognition::ChatMessage;
use crate::config::WebrtcConfig;
use crate::pipeline::Pipeline;
use crate::signaling::SignalingMessage;
use bytes::BytesMut;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer, RTCPeerConnectionIceEvent,
    RTCSessionDescription,
};

/// Maximum size of a single audio chunk sent over the data channel.
const AUDIO_CHUNK_SIZE: usize = 16 * 1024;

/// Send an Ogg Opus audio buffer over the data channel as a length-prefixed
/// sequence of chunks: a text header `audio:<total_bytes>` followed by binary
/// chunks. The channel is ordered and reliable, so the client reassembles by
/// concatenation.
async fn send_audio(channel: &Arc<dyn DataChannel>, audio: &[u8]) -> webrtc::error::Result<()> {
    channel.send_text(&format!("audio:{}", audio.len())).await?;
    for chunk in audio.chunks(AUDIO_CHUNK_SIZE) {
        let mut buf = BytesMut::with_capacity(chunk.len());
        buf.extend_from_slice(chunk);
        channel.send(buf).await?;
    }
    Ok(())
}

/// Shared ICE state: the candidates gathered so far, plus a flag set when
/// gathering completes.
struct IceState {
    candidates: Mutex<Vec<RTCIceCandidateInit>>,
    gathering_complete: Mutex<bool>,
}

impl Default for IceState {
    fn default() -> Self {
        Self {
            candidates: Mutex::new(Vec::new()),
            gathering_complete: Mutex::new(false),
        }
    }
}

/// Handles peer-connection events: collects ICE candidates, and on
/// `on_data_channel` runs the pipeline on each incoming text message.
struct Handler {
    pipeline: Arc<Pipeline>,
    ice: Arc<IceState>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(init) = event.candidate.to_json() {
            self.ice.candidates.lock().unwrap().push(init);
        }
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            *self.ice.gathering_complete.lock().unwrap() = true;
        }
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let pipeline = self.pipeline.clone();
        tokio::spawn(async move {
            while let Some(event) = data_channel.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8_lossy(&msg.data).to_string();
                        let messages = vec![ChatMessage::user(text)];
                        match pipeline.run(&messages).await {
                            Ok(out) => {
                                if let Err(e) = send_audio(&data_channel, &out.audio).await {
                                    eprintln!("failed to send audio over data channel: {e}");
                                }
                            }
                            Err(e) => {
                                eprintln!("pipeline error: {e}");
                            }
                        }
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        });
    }
}

/// The WebRTC server: a single peer connection that answers one offer and
/// serves the voice loop over its data channel.
pub struct WebRtcServer {
    pc: Arc<dyn PeerConnection>,
    ice: Arc<IceState>,
}

impl WebRtcServer {
    /// Build the server: a peer connection bound to `0.0.0.0:{listen_port}`
    /// with the configured STUN servers for ICE candidate gathering (host-only
    /// candidates are not reachable across NAT).
    pub async fn new(
        config: &WebrtcConfig,
        pipeline: Arc<Pipeline>,
    ) -> webrtc::error::Result<Self> {
        let ice = Arc::new(IceState::default());
        let handler = Arc::new(Handler {
            pipeline,
            ice: ice.clone(),
        });
        let ice_servers = config
            .stun_servers
            .iter()
            .map(|url| RTCIceServer {
                urls: vec![url.clone()],
                ..Default::default()
            })
            .collect();
        let rtc_config = RTCConfigurationBuilder::default()
            .with_ice_servers(ice_servers)
            .build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(rtc_config)
            .with_handler(handler)
            .with_udp_addrs(vec![format!("0.0.0.0:{}", config.listen_port)])
            .build()
            .await?;
        Ok(Self {
            pc: Arc::new(pc),
            ice,
        })
    }

    /// Answer an offer: set the remote description, add the client's ICE
    /// candidates, create the answer, set it as the local description, wait
    /// for gathering to complete, and return the answer plus the server's own
    /// candidates.
    pub async fn answer(
        &self,
        offer: RTCSessionDescription,
        remote_candidates: Vec<RTCIceCandidateInit>,
    ) -> webrtc::error::Result<SignalingMessage> {
        self.pc.set_remote_description(offer).await?;
        for candidate in remote_candidates {
            self.pc.add_ice_candidate(candidate).await?;
        }
        let answer = self.pc.create_answer(None).await?;
        self.pc.set_local_description(answer.clone()).await?;
        wait_for_gathering(&self.ice).await;
        let candidates = self.ice.candidates.lock().unwrap().clone();
        Ok(SignalingMessage {
            description: answer,
            candidates,
        })
    }
}

/// Wait (up to 5 s) for ICE gathering to complete.
async fn wait_for_gathering(ice: &Arc<IceState>) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if *ice.gathering_complete.lock().unwrap() {
            return;
        }
        if tokio::time::Instant::now() > deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn server_builds_from_default_config() {
        let config = Config::default();
        let pipeline = Arc::new(Pipeline::new(&config));
        let server = WebRtcServer::new(&config.webrtc, pipeline)
            .await
            .expect("server should build with host-only ICE");
        // The peer connection exists and is in a pre-negotiation state.
        assert!(server.pc.local_description().await.is_none());
    }

    #[test]
    fn chunking_splits_audio_into_16kib_pieces() {
        let audio = vec![0xABu8; 76 * 1024];
        let chunks: Vec<&[u8]> = audio.chunks(AUDIO_CHUNK_SIZE).collect();
        assert_eq!(chunks.len(), 5);
        assert!(chunks.iter().all(|c| c.len() <= AUDIO_CHUNK_SIZE));
        let reassembled: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
        assert_eq!(reassembled, audio);
    }
}
