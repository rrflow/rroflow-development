//! Verification harness: feed received event fixtures to the production projector.
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = vec![];
    io::stdin()
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("fixture input exceeds 16 MiB".into());
    }
    let events = serde_json::from_slice::<Vec<rroflow_core::capture::CaptureEvent>>(&bytes)?;
    let activities = rroflow_core::activity::project(&events)?;
    println!("{}", serde_json::to_string(&activities)?);
    Ok(())
}
