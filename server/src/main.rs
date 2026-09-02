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
    // Install the rustls crypto provider (ring) before any TLS use. The
    // `webrtc` crate already pulls in rustls with the `ring` feature, so we
    // match it here to avoid a provider-ambiguity panic.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.yaml".to_string());

    let config = Config::load(&path)?;
    if config.signaling.auth_token.is_empty() {
        eprintln!(
            "WARNING: signaling.auth_token is empty — the signaling endpoint is unauthenticated. Set a non-empty token before exposing the server to the internet."
        );
    }
    let pipeline = Arc::new(Pipeline::new(&config));
    let server = Arc::new(WebRtcServer::new(&config.webrtc, pipeline).await?);

    let app = signaling::router(server, config.signaling.auth_token.clone());
    let addr = format!("0.0.0.0:{}", config.signaling.listen_port);

    if config.signaling.tls.enabled() {
        // HTTPS mode: serve the signaling endpoint over TLS using the
        // configured certificate chain and private key.
        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            &config.signaling.tls.cert,
            &config.signaling.tls.key,
        )
        .await?;
        let socket_addr: std::net::SocketAddr = addr.parse()?;
        println!("signaling listening on https://{addr}");
        axum_server::bind_rustls(socket_addr, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        // Plain HTTP (localhost dev).
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        println!("signaling listening on http://{addr}");
        axum::serve(listener, app).await?;
    }
    Ok(())
}
