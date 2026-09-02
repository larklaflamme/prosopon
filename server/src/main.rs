//! Prosopon server entry point.
//!
//! Slice 1: loads `config.yaml` and prints the resolved configuration.
//! Later slices wire up the WebRTC server, cognition, and TTS pipeline.

use prosopon_server::config::Config;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.yaml".to_string());

    match Config::load(&path) {
        Ok(cfg) => {
            println!("loaded config from {path}:");
            println!("  tts.base_url      = {}", cfg.tts.base_url);
            println!("  tts.model         = {}", cfg.tts.model);
            println!("  tts.voice         = {}", cfg.tts.voice);
            println!("  cognition.base_url = {}", cfg.cognition.base_url);
            println!("  cognition.model    = {}", cfg.cognition.model);
            println!("  webrtc.listen_port = {}", cfg.webrtc.listen_port);
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
