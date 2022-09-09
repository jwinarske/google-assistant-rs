use git2::{ErrorCode, Repository};
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {

    if !Path::new("proto/google/assistant/embedded/v1alpha2/embedded_assistant.proto").exists() {

        let googleapis = "proto";

        match Repository::clone(
            "https://github.com/googleapis/googleapis.git",
            std::path::Path::new(googleapis),
        ) {
            Ok(_) => {
                println!("[{}] cloned", googleapis);
            }
            Err(e) => match e.code() {
                ErrorCode::Exists => println!("[{}] exists", googleapis),
                _ => panic!(
                    "[{}] unexpected: {:?} {:?}",
                    googleapis,
                    e.code(),
                    e.message()
                ),
            },
        }
    }

    fs::remove_dir_all("src/api")?;
    fs::create_dir("src/api")?;

    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .format(true)
        .out_dir("src/api")
        .compile(
            &["proto/google/assistant/embedded/v1alpha2/embedded_assistant.proto"],
            &["proto"],
        )?;
    Ok(())
}