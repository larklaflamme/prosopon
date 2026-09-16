//! Prosopon Audio2Face-3D bridge (Phase 1).
//!
//! Streams a PCM-16 mono WAV file to the self-hosted Audio2Face-3D NIM
//! (gRPC `ProcessAudioStream` on localhost:52000) and emits the resulting
//! ARKit blendshape frames as JSON lines on stdout.
//!
//! Output format:
//!   line 1: {"header":{"blendShapes":["EyeBlinkLeft", ...]}}
//!   then:   {"t":0.033,"v":[0.0, ...]}   (one line per animation frame)
//!
//! Usage:
//!   a2f-bridge <input.wav> [endpoint]   (endpoint defaults to http://localhost:52000)

use std::error::Error;

use a2f_bridge::nvidia_ace;

use tonic::Request;

use nvidia_ace::a2f::v1::{
    BlendShapeParameters, EmotionPostProcessingParameters, FaceParameters,
};
use nvidia_ace::audio::v1::AudioHeader;
use nvidia_ace::controller::v1::{
    audio_stream, AudioStream, AudioStreamHeader,
};
use nvidia_ace::services::a2f_controller::v1::a2f_controller_service_client::A2fControllerServiceClient;

/// Default face/emotion/blendshape parameters (claire_v2.3 defaults).
/// These are the tunables the future client settings sliders will bind to.
fn default_face_params() -> FaceParameters {
    let mut fp = std::collections::HashMap::new();
    fp.insert("upperFaceStrength".to_string(), 1.0f32);
    fp.insert("upperFaceSmoothing".to_string(), 0.001f32);
    fp.insert("lowerFaceStrength".to_string(), 1.25f32);
    fp.insert("lowerFaceSmoothing".to_string(), 0.006f32);
    fp.insert("faceMaskLevel".to_string(), 0.6f32);
    fp.insert("faceMaskSoftness".to_string(), 0.0085f32);
    fp.insert("skinStrength".to_string(), 1.0f32);
    fp.insert("eyelidOpenOffset".to_string(), 0.0f32);
    fp.insert("lipOpenOffset".to_string(), 0.0f32);
    fp.insert("tongueStrength".to_string(), 1.3f32);
    fp.insert("tongueHeightOffset".to_string(), 0.0f32);
    fp.insert("tongueDepthOffset".to_string(), 0.0f32);
    FaceParameters {
        float_params: fp,
        integer_params: Default::default(),
        float_array_params: Default::default(),
    }
}

fn default_emotion_params() -> EmotionPostProcessingParameters {
    EmotionPostProcessingParameters {
        emotion_contrast: Some(1.0),
        live_blend_coef: Some(0.7),
        enable_preferred_emotion: Some(false),
        preferred_emotion_strength: Some(0.5),
        emotion_strength: Some(0.6),
        max_emotions: Some(3),
    }
}

fn default_blendshape_params() -> BlendShapeParameters {
    // Neutral multipliers (1.0) and offsets (0.0) — the model's raw output.
    // The claire config applies per-shape multipliers; we start neutral and
    // tune via the settings sliders later.
    BlendShapeParameters {
        bs_weight_multipliers: Default::default(),
        bs_weight_offsets: Default::default(),
        enable_clamping_bs_weight: Some(false),
    }
}

/// Build the stream of `AudioStream` messages: header, then audio chunks, then end-of-audio.
fn build_request_stream(
    samples: Vec<i16>,
    sample_rate: u32,
    chunk_samples: usize,
) -> impl Iterator<Item = AudioStream> {
    let header = AudioStream {
        stream_part: Some(audio_stream::StreamPart::AudioStreamHeader(
            AudioStreamHeader {
                audio_header: Some(AudioHeader {
                    audio_format: 0, // AUDIO_FORMAT_PCM
                    channel_count: 1,
                    samples_per_second: sample_rate,
                    bits_per_sample: 16,
                }),
                face_params: Some(default_face_params()),
                emotion_post_processing_params: Some(default_emotion_params()),
                blendshape_params: Some(default_blendshape_params()),
                emotion_params: None,
            },
        )),
    };

    let mut messages = Vec::new();
    messages.push(header);

    for chunk in samples.chunks(chunk_samples) {
        // Convert i16 samples to little-endian bytes.
        let mut buf = Vec::with_capacity(chunk.len() * 2);
        for s in chunk {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        messages.push(AudioStream {
            stream_part: Some(audio_stream::StreamPart::AudioWithEmotion(
                nvidia_ace::a2f::v1::AudioWithEmotion {
                    audio_buffer: buf,
                    emotions: Vec::new(),
                },
            )),
        });
    }

    messages.push(AudioStream {
        stream_part: Some(audio_stream::StreamPart::EndOfAudio(
            audio_stream::EndOfAudio {},
        )),
    });

    messages.into_iter()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: a2f-bridge <input.wav> [endpoint]");
        std::process::exit(1);
    }
    let wav_path = &args[1];
    let endpoint = if args.len() >= 3 {
        args[2].clone()
    } else {
        "http://localhost:52000".to_string()
    };

    // Read the WAV (PCM-16 mono).
    let mut reader = hound::WavReader::open(wav_path)?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels;
    let bits = spec.bits_per_sample;
    eprintln!(
        "wav: {} Hz, {} ch, {} bits",
        sample_rate, channels, bits
    );
    if channels != 1 || bits != 16 {
        eprintln!("warning: A2F expects PCM-16 mono; got {} ch / {} bits", channels, bits);
    }
    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    eprintln!("read {} samples ({:.2}s)", samples.len(), samples.len() as f64 / sample_rate as f64);

    // Connect to the A2F NIM.
    let mut client = A2fControllerServiceClient::connect(endpoint.clone()).await?;
    eprintln!("connected to {}", endpoint);

    // 100ms chunks for low latency.
    let chunk_samples = (sample_rate as usize) / 10;

    let request_stream = tokio_stream::iter(build_request_stream(samples, sample_rate, chunk_samples));
    let response = client
        .process_audio_stream(Request::new(request_stream))
        .await?;
    let mut inbound = response.into_inner();

    // Read the stream: header (names once), then animation frames, then status.
    let mut frame_count = 0u64;
    while let Some(msg) = inbound.message().await? {
        use nvidia_ace::controller::v1::animation_data_stream::StreamPart;
        match msg.stream_part {
            Some(StreamPart::AnimationDataStreamHeader(h)) => {
                if let Some(skel) = h.skel_animation_header {
                    let names = skel.blend_shapes;
                    eprintln!("header: {} blendshapes", names.len());
                    let header_json = serde_json::json!({
                        "header": { "blendShapes": names }
                    });
                    println!("{}", header_json);
                }
            }
            Some(StreamPart::AnimationData(ad)) => {
                if let Some(skel) = ad.skel_animation {
                    for bsw in skel.blend_shape_weights {
                        frame_count += 1;
                        let frame = serde_json::json!({
                            "t": bsw.time_code,
                            "v": bsw.values,
                        });
                        println!("{}", frame);
                    }
                }
            }
            Some(StreamPart::Status(s)) => {
                eprintln!("status: code={} message={}", s.code, s.message);
            }
            Some(StreamPart::Event(_)) => {
                // END_OF_A2F_AUDIO_PROCESSING — informational.
            }
            None => {}
        }
    }

    eprintln!("done: {} animation frames", frame_count);
    Ok(())
}
