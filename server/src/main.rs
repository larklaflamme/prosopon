//! Prosopon server entry point.
//!
//! Loads `config.yaml`, builds the voice-loop pipeline and the WebRTC server,
//! and serves the HTTP signaling endpoint (Option B). The client POSTs its SDP
//! offer to `/offer`, receives the answer, and the data channel carries text
//! (client → server) and Ogg Opus audio (server → client).

use prosopon_server::config::Config;
use prosopon_server::pipeline::Pipeline;
use prosopon_server::signaling;
use prosopon_server::webrtc::WebRtcServer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.yaml".to_string());

    let config = Config::load(&path)?;
    let pipeline = Arc::new(Pipeline::new(&config));
    let server = Arc::new(WebRtcServer::new(&config.webrtc, pipeline).await?);

    let app = signaling::router(server);
    let addr = format!("0.0.0.0:{}", config.signaling.listen_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("signaling listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
