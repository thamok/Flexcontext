use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
fn main() {
    // Conservative compatibility boundary: source layout, extraction, grammar
    // resolution, normalization, postings and cache encoding all participate.
    let inputs = [
        "Cargo.lock",
        "src/parser.rs",
        "src/model.rs",
        "src/language.rs",
        "src/lexical.rs",
        "src/index.rs",
        "src/cache.rs",
        "build.rs",
    ];
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mut digest = Sha256::new();
    for input in inputs {
        println!("cargo:rerun-if-changed={input}");
        let bytes = fs::read(root.join(input)).unwrap();
        digest.update((input.len() as u64).to_le_bytes());
        digest.update(input.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    println!(
        "cargo:rustc-env=FLEXCONTEXT_CACHE_FINGERPRINT={:x}",
        digest.finalize()
    );
}
