use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    // OUT_DIR is target/{profile}/build/{crate}-{hash}/out
    // Walk up to target/{profile}/
    let target_profile_dir: PathBuf = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("target profile dir")
        .to_path_buf();

    let dest = target_profile_dir.join("lists");
    fs::create_dir_all(&dest).expect("create lists/");

    // Repo root is two up from this crate (crates/client/)
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lists");
    println!("cargo:rerun-if-changed={}", src.display());

    if let Ok(entries) = fs::read_dir(&src) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("txt") {
                let name = p.file_name().expect("filename");
                fs::copy(&p, dest.join(name)).expect("copy list file");
            }
        }
    }
}
