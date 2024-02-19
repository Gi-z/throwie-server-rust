// extern crate protobuf;
// extern crate protobuf_codegen;
//
// fn main() {
//     protobuf_codegen::Codegen::new()
//         .protoc()
//         .includes(&["src/proto"])
//         .input("src/proto/csimsg.proto")
//         .input("src/proto/telemetrymsg.proto")
//         .cargo_out_dir("proto")
//         .run_from_script();
// }

extern crate prost_build;
fn main() {
    prost_build::compile_protos(
        &[
            "src/proto/csimsg.proto",
            "src/proto/telemetrymsg.proto"
        ],
        &["src/proto"]).unwrap();
}