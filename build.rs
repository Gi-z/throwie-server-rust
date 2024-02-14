extern crate protobuf;
extern crate protobuf_codegen;

fn main() {
    protobuf_codegen::Codegen::new()
        .protoc()
        .includes(&["src/proto"])
        .input("src/proto/csimsg.proto")
        .input("src/proto/telemetrymsg.proto")
        .cargo_out_dir("proto")
        .run_from_script();
}