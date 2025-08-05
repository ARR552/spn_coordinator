extern crate prost_build;
extern crate tonic_prost_build;

#[allow(deprecated)]
fn main() {
    println!("cargo:rerun-if-changed=../../../proto");
    let config = tonic_prost_build::configure();
    config
        .protoc_arg("--experimental_allow_proto3_optional")
        .out_dir("src/generated")
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .type_attribute(".network.ProofStatus", "#[derive(sqlx::Type)]")
        .file_descriptor_set_path("src/generated/descriptor.bin")
        .compile_protos(
            &["../../../proto/types.proto", "../../../proto/network.proto", "../../../proto/artifact.proto", "../../../proto/verifier.proto"],
            &["../../../proto"],
        )
        .unwrap();
}