use std::{io, path::Path};

/// Preserve the bytes SQLx embeds and checks against applied migrations.
/// Reject altered checkouts rather than silently normalizing one hash path.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    if bytes.contains(&b'\r') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} contains CR line endings; build from an LF checkout as required by .gitattributes. Do not change applied database checksums.",
                path.display()
            ),
        ));
    }
    Ok(bytes)
}
