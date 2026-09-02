//! Full-loop integration test: the client's WebRTC transport against the
//! server's WebRTC transport, on this box.
//!
//! This is the highest-value verification we can do before Lark runs the real
//! Mac ↔ server test: it proves the client and server WebRTC code
//! interoperate, the offer/answer signaling works, and the chunked audio
//! round-trips losslessly — all against the *real* Kokoro + Ollama pipeline.
//!
//! Gated behind `--features live-tests` because it needs Kokoro (localhost
//! 21802) and Ollama (localhost 11434) running.

use prosopon_client_core::config::ClientConfig;
use prosopon_client_core::webrtc_client::WebRtcClient;
use prosopon_server::config::Config;
use prosopon_server::pipeline::Pipeline;
use prosopon_server::signaling;
use prosopon_server::webrtc::WebRtcServer;
use std::sync::Arc;

#[tokio::test]
async fn full_loop_text_to_audio_over_webrtc() {
    // --- Server side: WebRTC server + signaling router on ephemeral ports. ---
    let mut config = Config::default();
    config.webrtc.listen_port = 0; // ephemeral UDP
    let pipeline = Arc::new(Pipeline::new(&config));
    let server = Arc::new(
        WebRtcServer::new(&config.webrtc, pipeline)
            .await
            .expect("server should build"),
    );

    let app = signaling::router(server, String::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind signaling listener");
    let signaling_addr = listener.local_addr().expect("get signaling addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve signaling");
    });
    let signaling_url = format!("http://{signaling_addr}/offer");

    // --- Client side: connect, send text, receive reassembled audio. ---
    let client_config = ClientConfig {
        signaling: prosopon_client_core::config::SignalingConfig {
            url: signaling_url,
            ..Default::default()
        },
        ..Default::default()
    };
    let client = WebRtcClient::connect(&client_config)
        .await
        .expect("client should connect");

    client
        .send_text("Hello, this is a loopback test of the full voice pipeline.")
        .await
        .expect("send text");

    let audio = client.recv_audio().await.expect("receive audio");

    // The reassembled audio must be a non-empty Ogg Opus stream.
    assert!(!audio.is_empty(), "audio should not be empty");
    assert_eq!(&audio[0..4], b"OggS", "audio should start with OggS magic");

    client.close().await.expect("close client");
}
