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
//! NOTE (2026-09-02): the `webrtc` crate was rewritten since the 0.11-era API
//! this project was originally planned against. This module targets the new
//! 0.20.x API: `PeerConnectionBuilder`, the `PeerConnection` trait, the
//! `DataChannel` trait with `poll()`, and the `PeerConnectionEventHandler`
//! trait. See `design/14-server-implementation-plan.md` §6.

use crate::cognition::ChatMessage;
use crate::config::WebrtcConfig;
use crate::pipeline::Pipeline;
use bytes::BytesMut;
use std::sync::Arc;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCSessionDescription,
};

/// Handles peer-connection events. On `on_data_channel`, spawns a task that
/// polls the channel and runs the pipeline on each incoming text message.
#[derive(Clone)]
struct Handler {
    pipeline: Arc<Pipeline>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
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
                                let mut buf = BytesMut::with_capacity(out.audio.len());
                                buf.extend_from_slice(&out.audio);
                                if let Err(e) = data_channel.send(buf).await {
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
}

impl WebRtcServer {
    /// Build the server: a peer connection bound to `0.0.0.0:{listen_port}`
    /// with host-only ICE (no STUN/TURN — the plan's no-ICE mode, intended to
    /// run over an SSH tunnel).
    pub async fn new(
        config: &WebrtcConfig,
        pipeline: Arc<Pipeline>,
    ) -> webrtc::error::Result<Self> {
        let handler = Arc::new(Handler { pipeline });
        let rtc_config = RTCConfigurationBuilder::default().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(rtc_config)
            .with_handler(handler)
            .with_udp_addrs(vec![format!("0.0.0.0:{}", config.listen_port)])
            .build()
            .await?;
        Ok(Self { pc: Arc::new(pc) })
    }

    /// Answer an offer: set the remote description, create the answer, and set
    /// it as the local description. Returns the answer for the caller to send
    /// back to the client over the signaling channel.
    pub async fn answer(
        &self,
        offer: RTCSessionDescription,
    ) -> webrtc::error::Result<RTCSessionDescription> {
        self.pc.set_remote_description(offer).await?;
        let answer = self.pc.create_answer(None).await?;
        self.pc.set_local_description(answer.clone()).await?;
        Ok(answer)
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
}
