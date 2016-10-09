use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    File::create(Path::new(env!("OUT_DIR")).join("version.txt"))
        .unwrap()
        .write_all(env!("CARGO_PKG_VERSION").as_bytes())
        .unwrap();
}
