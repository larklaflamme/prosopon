//! Live integration test against the real Kokoro TTS service.
//! Gated behind `--features live-tests` (see Cargo.toml `required-features`).

use prosopon_server::config::Config;
use prosopon_server::tts::TtsClient;

#[tokio::test]
async fn synthesize_returns_ogg_opus() {
    let cfg = Config::load(concat!(env!("CARGO_MANIFEST_DIR"), "/config.yaml"))
        .expect("shipped config.yaml should parse");
    let client = TtsClient::new(&cfg.tts);

    let bytes = client
        .synthesize("Hello there. This is a test.")
        .await
        .expect("live Kokoro should synthesize");

    // Non-empty.
    assert!(!bytes.is_empty(), "audio should not be empty");

    // Ogg magic bytes ("OggS").
    assert_eq!(&bytes[..4], b"OggS", "audio should be an Ogg container");

    // OpusHead present (one per Ogg stream).
    let head = b"OpusHead";
    assert!(
        bytes.windows(head.len()).any(|w| w == head),
        "Ogg stream should contain an OpusHead"
    );
}
