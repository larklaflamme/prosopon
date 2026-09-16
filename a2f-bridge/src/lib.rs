//! Prosopon Audio2Face-3D bridge library.
//!
//! Exposes the generated gRPC types under `nvidia_ace::...` so the bridge
//! binary (and later, the server integration) can talk to the A2F NIM.

pub mod nvidia_ace {
    pub mod controller {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.controller.v1");
        }
    }
    pub mod a2f {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.a2f.v1");
        }
    }
    pub mod audio {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.audio.v1");
        }
    }
    pub mod animation_data {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.animation_data.v1");
        }
    }
    pub mod animation_id {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.animation_id.v1");
        }
    }
    pub mod status {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.status.v1");
        }
    }
    pub mod emotion_with_timecode {
        pub mod v1 {
            tonic::include_proto!("nvidia_ace.emotion_with_timecode.v1");
        }
    }
    pub mod services {
        pub mod a2f_controller {
            pub mod v1 {
                tonic::include_proto!("nvidia_ace.services.a2f_controller.v1");
            }
        }
    }
}
