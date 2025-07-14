extern crate prost_build;
fn main() {
    prost_build::compile_protos(
        &[
            "./src/proto/csimsg.proto",
            "./src/proto/telemetrymsg.proto",
            "./src/proto/bme280reading.proto",
            "./src/proto/pirreading.proto"
        ],
        &["./src/proto"])
        .expect("error compiling protobuf files");
}
