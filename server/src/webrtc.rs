//! WebRTC data-channel server (Slice 5).
//!
//! Under Option B (Lark's decision, 2026-09-02): the server ships WAV
//! bytes as-is over the WebRTC **data channel** — no audio track, no
//! transcoding, no demuxing. The client plays the WAV natively.
//!
//! The server accepts one peer. The client creates the data channel; the
//! server receives it via `on_data_channel`, polls it for text messages,
//! runs each through the pipeline (cognition + TTS), and sends the resulting
//! WAV bytes back over the same channel.
//!
//! ## Audio chunking (2026-09-02)
//!
//! The SCTP data channel's default max message size is 64 KiB (RFC 8841).
//! Kokoro's WAV output for a typical sentence is larger than Opus (measured
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
    RTCPeerConnectionState, RTCSessionDescription,
};

/// Maximum size of a single audio chunk sent over the data channel.
const AUDIO_CHUNK_SIZE: usize = 16 * 1024;

/// Send an audio buffer over the data channel as a length-prefixed
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

/// Send a blendshape track (NDJSON text) over the blendshapes data channel as
/// a length-prefixed sequence of chunks: a text header `blendshapes:<n>`
/// followed by binary chunks of the UTF-8 bytes. Mirrors `send_audio` so the
/// client can reuse the same reassembly logic.
async fn send_blendshapes(channel: &Arc<dyn DataChannel>, track: &str) -> webrtc::error::Result<()> {
    channel.send_text(&format!("blendshapes:{}", track.len())).await?;
    for chunk in track.as_bytes().chunks(AUDIO_CHUNK_SIZE) {
        let mut buf = BytesMut::with_capacity(chunk.len());
        buf.extend_from_slice(chunk);
        channel.send(buf).await?;
    }
    Ok(())
}

/// Wait for a data channel to open, bounded by `timeout`. Returns `true` if it
/// opened, `false` on close/error/timeout.
async fn wait_for_open(channel: &Arc<dyn DataChannel>, timeout: Duration) -> bool {
    let result = tokio::time::timeout(timeout, async {
        loop {
            match channel.poll().await {
                Some(DataChannelEvent::OnOpen) => return true,
                Some(DataChannelEvent::OnClose) | Some(DataChannelEvent::OnError) | None => {
                    return false;
                }
                Some(_) => {}
            }
        }
    })
    .await;
    result.unwrap_or(false)
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
    connection_state: Arc<Mutex<RTCPeerConnectionState>>,
    /// Populated after the peer connection is built; lets the data-channel
    /// handler close the pc when the client disconnects.
    pc: Arc<Mutex<Option<Arc<dyn PeerConnection>>>>,
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

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        *self.connection_state.lock().unwrap() = state;
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let pipeline = self.pipeline.clone();
        let pc = self.pc.clone();

        // Create the server-initiated blendshapes channel (the second data
        // channel on the same peer connection — no new network surface, no new
        // port). If it fails to open, `blendshapes_channel` is `None` and the
        // avatar degrades to audio-only.
        let blendshapes_channel = {
            // Clone the Arc out of the mutex and drop the guard before the
            // `.await` below (a MutexGuard is not Send).
            let pc_clone = pc.lock().unwrap().clone();
            match pc_clone {
                Some(pc) => match pc.create_data_channel("blendshapes", None).await {
                    Ok(ch) if wait_for_open(&ch, Duration::from_secs(10)).await => Some(ch),
                    Ok(_) => {
                        eprintln!("blendshapes channel failed to open; avatar disabled");
                        None
                    }
                    Err(e) => {
                        eprintln!("failed to create blendshapes channel: {e}");
                        None
                    }
                },
                None => None,
            }
        };

        tokio::spawn(async move {
            // Per-session conversational history. Each data channel is one
            // client session, so the history lives here and accumulates
            // across turns (user + assistant messages).
            let mut history: Vec<ChatMessage> = Vec::new();
            while let Some(event) = data_channel.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8_lossy(&msg.data).to_string();
                        history.push(ChatMessage::user(text));
                        match pipeline.run(&history).await {
                            Ok(out) => {
                                history.push(ChatMessage::assistant(out.reply.clone()));
                                if let Err(e) = send_audio(&data_channel, &out.audio).await {
                                    eprintln!("failed to send audio over data channel: {e}");
                                }
                                if let (Some(ch), Some(bs)) = (&blendshapes_channel, &out.blendshapes) {
                                    if let Err(e) = send_blendshapes(ch, bs).await {
                                        eprintln!("failed to send blendshapes over data channel: {e}");
                                    }
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
            // The client is gone: close our peer connection so the signaling
            // layer can prune this session and free its UDP port for the next
            // connection.
            let pc_to_close = pc.lock().unwrap().clone();
            if let Some(pc) = pc_to_close {
                let _ = pc.close().await;
            }
        });
    }
}

/// The WebRTC server: a single peer connection that answers one offer and
/// serves the voice loop over its data channel.
pub struct WebRtcServer {
    pc: Arc<dyn PeerConnection>,
    ice: Arc<IceState>,
    connection_state: Arc<Mutex<RTCPeerConnectionState>>,
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
        let connection_state = Arc::new(Mutex::new(RTCPeerConnectionState::New));
        let pc_handle: Arc<Mutex<Option<Arc<dyn PeerConnection>>>> = Arc::new(Mutex::new(None));
        let handler = Arc::new(Handler {
            pipeline,
            ice: ice.clone(),
            connection_state: connection_state.clone(),
            pc: pc_handle.clone(),
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
        let pc: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_configuration(rtc_config)
                .with_handler(handler)
                .with_udp_addrs(vec![format!("0.0.0.0:{}", config.listen_port)])
                .build()
                .await?,
        );
        // Give the data-channel handler a handle to the pc so it can close
        // the connection when the client disconnects.
        *pc_handle.lock().unwrap() = Some(pc.clone());
        Ok(Self {
            pc,
            ice,
            connection_state,
        })
    }

    /// The peer connection's current state, tracked via
    /// `on_connection_state_change`. Used by the signaling layer to prune
    /// dead sessions.
    pub fn connection_state(&self) -> RTCPeerConnectionState {
        *self.connection_state.lock().unwrap()
    }

    /// Close the peer connection, releasing its UDP socket. Dropping the
    /// `WebRtcServer` alone does NOT free the socket: the pc holds the
    /// handler, and the handler holds a clone of the pc (a reference cycle),
    /// so the socket stays bound until `close()` is called explicitly. The
    /// signaling layer calls this when pruning a dead session so the fixed
    /// `listen_port` is freed for the next client.
    pub async fn close(&self) {
        let _ = self.pc.close().await;
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
