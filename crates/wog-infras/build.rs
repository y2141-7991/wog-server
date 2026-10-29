fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A vendored protoc, so building needs no system install.
    // SAFETY: build scripts are single-threaded.
    unsafe { std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?) };
    tonic_prost_build::compile_protos("proto/crawler.proto")?;
    Ok(())
}
