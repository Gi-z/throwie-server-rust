use protobuf_codegen::Codegen;

fn main() {
    Codegen::new()
        .protoc()
        .cargo_out_dir("proto")
        .input("src/proto/csimsg.proto")
        .include("src/proto")
        .run_from_script();
}