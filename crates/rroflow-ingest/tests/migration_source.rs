#[path = "../migration_source.rs"]
mod migration_source;

#[test]
fn canonical_bytes_are_preserved_and_changed_line_endings_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fixture.sql");
    let original = b"-- fixture\nSELECT 'unchanged';\n";
    std::fs::write(&path, original).unwrap();
    assert_eq!(migration_source::read(&path).unwrap(), original);
    for altered in [b"SELECT 1;\r\n".as_slice(), b"SELECT 1;\r"] {
        std::fs::write(&path, altered).unwrap();
        let error = migration_source::read(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("LF checkout"));
        // The guard never rewrites source files or SQLx's stored checksums.
        assert_eq!(std::fs::read(&path).unwrap(), altered);
    }
}

#[test]
fn every_migration_uses_the_canonical_source_bytes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "sql") {
            assert_eq!(
                migration_source::read(&path).unwrap(),
                std::fs::read(path).unwrap()
            );
        }
    }
}
