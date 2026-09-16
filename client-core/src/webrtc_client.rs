//! WebRTC data-channel client — the mirror of the server's `webrtc.rs`.
//!
//! The client is the *initiating* peer: it creates the data channel, creates
//! the offer, gathers its ICE candidates, exchanges the offer + candidates
//! for the server's answer + candidates over HTTP signaling, and then sends
//! text and receives chunked Ogg Opus audio over the channel.
//!
//! ## ICE candidate exchange
//!
//! The `webrtc` crate uses trickle ICE: candidates are gathered asynchronously
//! and delivered via `on_ice_candidate`, not embedded in the SDP. The client
//! therefore waits for gathering to complete, then sends its offer + all its
//! candidates in one signaling round-trip, and adds the server's candidates
//! via `add_ice_candidate`.
//!
//! ## Audio reassembly
//!
//! The server sends audio as a text header `audio:<total_bytes>` followed by
//! binary chunks (see the server's `webrtc.rs`). The client reads the header,
//! then accumulates binary messages until it has `total_bytes` bytes.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer, RTCPeerConnectionIceEvent,
};

use crate::config::ClientConfig;
use crate::signaling::{SignalingClient, SignalingMessage};
use crate::ClientError;

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

/// Collects ICE candidates.
struct Handler {
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
}

/// The WebRTC client: a single peer connection that initiates the voice loop.
pub struct WebRtcClient {
    pc: Arc<dyn PeerConnection>,
    data_channel: Arc<dyn DataChannel>,
    blendshapes_channel: Arc<dyn DataChannel>,
}

impl WebRtcClient {
    /// Connect to the server: build the peer connection, create the data
    /// channel, perform the offer/answer + candidate exchange over
    /// `config.signaling.url`, and wait for the data channel to open.
    pub async fn connect(config: &ClientConfig) -> Result<Self, ClientError> {
        let ice = Arc::new(IceState::default());
        let handler = Arc::new(Handler {
            ice: ice.clone(),
        });

        let ice_servers = config
            .webrtc
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
            .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
            .build()
            .await?;

        // Create both data channels (client-initiated, announced in-band).
        // The "voice" channel carries the conversation; the "blendshapes"
        // channel carries the avatar track. Both must be created before the
        // offer so they negotiate in the initial handshake (the server cannot
        // create a channel after answering without renegotiation).
        let data_channel = pc.create_data_channel("voice", None).await?;
        let blendshapes_channel = pc.create_data_channel("blendshapes", None).await?;

        // Create the offer and set it as the local description (starts
        // gathering).
        let offer = pc.create_offer(None).await?;
        pc.set_local_description(offer.clone()).await?;

        // Wait for gathering to complete, then collect our candidates.
        wait_for_gathering(&ice).await;
        let candidates = ice.candidates.lock().unwrap().clone();

        // Exchange the offer + candidates for the answer + candidates.
        let signaling =
            SignalingClient::new(&config.signaling.url, &config.signaling.auth_token);
        let answer = signaling
            .offer(SignalingMessage {
                description: offer,
                candidates,
            })
            .await?;

        pc.set_remote_description(answer.description).await?;
        for candidate in answer.candidates {
            pc.add_ice_candidate(candidate).await?;
        }

        // Wait for both data channels to open before returning.
        wait_for_open(&data_channel).await?;
        wait_for_open(&blendshapes_channel).await?;

        Ok(Self {
            pc: Arc::new(pc),
            data_channel,
            blendshapes_channel,
        })
    }

    /// Send the user's utterance as a text message.
    pub async fn send_text(&self, text: &str) -> Result<(), ClientError> {
        self.data_channel.send_text(text).await?;
        Ok(())
    }

    /// Receive one audio response: read the `audio:<n>` header, then
    /// accumulate binary chunks until `n` bytes have arrived. Returns the
    /// reassembled Ogg Opus buffer.
    pub async fn recv_audio(&self) -> Result<Vec<u8>, ClientError> {
        let total = self.read_audio_header().await?;
        let mut audio = Vec::with_capacity(total);
        while audio.len() < total {
            match self.data_channel.poll().await {
                Some(DataChannelEvent::OnMessage(msg)) if !msg.is_string => {
                    audio.extend_from_slice(&msg.data);
                }
                Some(DataChannelEvent::OnClose) | None => {
                    return Err(ClientError::ChannelClosed);
                }
                Some(_) => {}
            }
        }
        Ok(audio)
    }

    /// Read the `audio:<total_bytes>` header message.
    async fn read_audio_header(&self) -> Result<usize, ClientError> {
        loop {
            match self.data_channel.poll().await {
                Some(DataChannelEvent::OnMessage(msg)) if msg.is_string => {
                    let text = String::from_utf8_lossy(&msg.data);
                    if let Some(n) = text.strip_prefix("audio:") {
                        return n
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| ClientError::InvalidAudioHeader(text.to_string()));
                    }
                }
                Some(DataChannelEvent::OnClose) | None => {
                    return Err(ClientError::ChannelClosed);
                }
                Some(_) => {}
            }
        }
    }

    /// Receive one blendshape track: read the `blendshapes:<n>` header, then
    /// accumulate binary chunks until `n` bytes have arrived. Returns the
    /// reassembled NDJSON track (UTF-8 bytes). Returns `ChannelNotOpen` if the
    /// server never created the blendshapes channel (avatar disabled).
    pub async fn recv_blendshapes(&self) -> Result<Vec<u8>, ClientError> {
        let channel = &self.blendshapes_channel;
        let total = self.read_blendshapes_header(channel).await?;
        let mut track = Vec::with_capacity(total);
        while track.len() < total {
            match channel.poll().await {
                Some(DataChannelEvent::OnMessage(msg)) if !msg.is_string => {
                    track.extend_from_slice(&msg.data);
                }
                Some(DataChannelEvent::OnClose) | None => {
                    return Err(ClientError::ChannelClosed);
                }
                Some(_) => {}
            }
        }
        Ok(track)
    }

    /// Read the `blendshapes:<total_bytes>` header message from the blendshapes
    /// channel.
    async fn read_blendshapes_header(
        &self,
        channel: &Arc<dyn DataChannel>,
    ) -> Result<usize, ClientError> {
        loop {
            match channel.poll().await {
                Some(DataChannelEvent::OnMessage(msg)) if msg.is_string => {
                    let text = String::from_utf8_lossy(&msg.data);
                    if let Some(n) = text.strip_prefix("blendshapes:") {
                        return n
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| ClientError::InvalidBlendshapesHeader(text.to_string()));
                    }
                }
                Some(DataChannelEvent::OnClose) | None => {
                    return Err(ClientError::ChannelClosed);
                }
                Some(_) => {}
            }
        }
    }

    /// Close the peer connection.
    pub async fn close(&self) -> Result<(), ClientError> {
        self.pc.close().await?;
        Ok(())
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

/// Wait for the data channel to open, with a 10-second timeout.
async fn wait_for_open(channel: &Arc<dyn DataChannel>) -> Result<(), ClientError> {
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match channel.poll().await {
                Some(DataChannelEvent::OnOpen) => return Ok(()),
                Some(DataChannelEvent::OnClose) | Some(DataChannelEvent::OnError) | None => {
                    return Err(ClientError::ChannelNotOpen);
                }
                Some(_) => {}
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(ClientError::ChannelOpenTimeout),
    }
}
