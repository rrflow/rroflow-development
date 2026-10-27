use sha2::{Digest, Sha256};

fn main() {
    println!("cargo:rerun-if-env-changed=RROFLOW_BUILD_REVISION");
    println!("cargo:rerun-if-changed=migrations");
    let revision = std::env::var("RROFLOW_BUILD_REVISION").unwrap_or_else(|_| "development".into());
    assert!(
        revision == "development"
            || (revision.len() == 40
                && revision
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "RROFLOW_BUILD_REVISION must be a full lowercase commit SHA"
    );
    let mut files: Vec<_> = std::fs::read_dir("migrations")
        .expect("migration directory")
        .map(|entry| entry.expect("migration entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        hash.update(
            path.file_name()
                .expect("filename")
                .to_str()
                .expect("UTF-8 name")
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(std::fs::read(path).expect("read migration"));
        hash.update([0]);
    }
    println!("cargo:rustc-env=RROFLOW_RELEASE_REVISION={revision}");
    println!(
        "cargo:rustc-env=RROFLOW_RELEASE_TARGET={}",
        std::env::var("TARGET").expect("Cargo target")
    );
    println!(
        "cargo:rustc-env=RROFLOW_MIGRATIONS_SHA256={}",
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
}
