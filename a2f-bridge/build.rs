fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the A2F controller service proto (pulls in all transitive imports).
    // We are a client only, so no server code.
    tonic_build::configure()
        .build_server(false)
        .compile_protos(
            &["proto/nvidia_ace.services.a2f_controller.v1.proto"],
            &["proto/"],
        )?;
    Ok(())
}
