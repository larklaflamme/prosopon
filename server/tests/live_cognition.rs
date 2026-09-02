//! Live integration test against the real Ollama cognition service.
//! Gated behind `--features live-tests` (see Cargo.toml `required-features`).

use prosopon_server::cognition::{ChatMessage, CognitionClient};
use prosopon_server::config::Config;

#[tokio::test]
async fn chat_returns_nonempty_reply() {
    let cfg = Config::load(concat!(env!("CARGO_MANIFEST_DIR"), "/config.yaml"))
        .expect("shipped config.yaml should parse");
    let client = CognitionClient::new(&cfg.cognition);

    let messages = vec![ChatMessage::user("Say hello in one short sentence.")];
    let reply = client
        .chat(&messages)
        .await
        .expect("live Ollama should reply");

    assert!(!reply.trim().is_empty(), "reply should not be empty");
}
