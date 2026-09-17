//! Finds the `unicode-width` source that Cargo fetched and makes its private
//! tables reachable from `main.rs`.
//!
//! The crate's `src/tables.rs` is generated code whose lookups are private. It
//! is copied into `OUT_DIR` with those items made `pub`, so that `main.rs` can
//! read them directly rather than a transcription of them -- which is the whole
//! point: the Meadow tables are then the crate's tables, not a copy that could
//! drift.
//!
//! The original is copied too, unchanged, for `main.rs` to fingerprint.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let crate_dir = upstream_dir();
    let tables = crate_dir.join("src/tables.rs");
    let text = std::fs::read_to_string(&tables)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", tables.display()));

    std::fs::write(out.join("tables_orig.rs"), &text).unwrap();
    std::fs::write(out.join("upstream.rs"), publicise(&text)).unwrap();

    println!("cargo:rustc-env=UPSTREAM_DIR={}", crate_dir.display());
    println!("cargo:rerun-if-changed={}", tables.display());
    println!("cargo:rerun-if-changed=Cargo.toml");
}

/// Where Cargo put the `unicode-width` this build depends on.
fn upstream_dir() -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("could not run `cargo metadata`");
    assert!(out.status.success(), "`cargo metadata` failed");
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pkg = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "unicode-width")
        .expect("unicode-width is not among the dependencies");
    Path::new(pkg["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_path_buf()
}

/// `text` with its module-level items made `pub`, so that they can be named
/// from outside the module it is included into.
fn publicise(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1024);
    for line in text.lines() {
        let rewritten = if line == "struct WidthInfo(u16);" {
            "pub struct WidthInfo(pub u16);".to_string()
        } else if let Some(rest) = line.strip_prefix("struct Align") {
            // `struct Align32<T>(T);`
            format!("pub struct Align{}", rest.replace("(T);", "(pub T);"))
        } else if line.starts_with("fn ")
            || line.starts_with("static ")
            || line.starts_with("const ")
        {
            format!("pub {line}")
        } else {
            line.to_string()
        };
        out.push_str(&rewritten);
        out.push('\n');
    }
    out
}
