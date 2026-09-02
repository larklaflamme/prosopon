//! Live pipeline test (Slice 4).
//!
//! Runs the full text → cognition → TTS → Ogg Opus pipeline against the
//! running Ollama + Kokoro services. Gated behind `--features live-tests`.

use prosopon_server::cognition::ChatMessage;
use prosopon_server::config::Config;
use prosopon_server::pipeline::Pipeline;

#[tokio::test]
async fn pipeline_returns_reply_and_ogg_opus() {
    let config = Config::default();
    let pipeline = Pipeline::new(&config);

    let messages = vec![ChatMessage::user("Say hello in one short sentence.")];
    let output = pipeline.run(&messages).await.expect("pipeline should run");

    // Reply text is non-empty.
    assert!(!output.reply.trim().is_empty(), "reply should be non-empty");

    // Audio is a valid Ogg Opus stream: OggS magic + OpusHead page.
    assert!(output.audio.len() > 4, "audio should not be empty");
    assert_eq!(&output.audio[0..4], b"OggS", "audio should start with OggS");
    assert!(
        output.audio.windows(8).any(|w| w == b"OpusHead"),
        "audio should contain an OpusHead page"
    );
}
