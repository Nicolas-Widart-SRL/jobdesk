//! Downloads every file first-run setup pins for Python, Bun and LaTeX, on both processors, and checks each against its
//! pinned SHA-256 and size. It needs the network, about 230 MB of it, and starts no `claude`:
//!
//! ```sh
//! cargo test --test live_downloads -- --ignored --nocapture
//! ```

use std::collections::BTreeSet;
use std::io::{self, Read};
use std::time::Instant;

use jobdesk::onboarding::application::{Arch, Artifact, HELPERS};
use jobdesk::shared_kernel::https::{Fetch, HttpsTransport, Transport};
use sha2::{Digest, Sha256};

/// The size and SHA-256 of what `url` serves, read as it arrives.
fn served(transport: &HttpsTransport, url: &str) -> Result<(u64, String), String> {
    let mut body = transport.open(url, Fetch::Download).map_err(|failure| format!("{failure:?}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut size = 0;
    loop {
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.kind().to_string()),
        };
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()))
}

fn check(transport: &HttpsTransport, artifact: &Artifact) -> Result<(), String> {
    let start = Instant::now();
    let (size, sha256) = served(transport, &artifact.url)?;
    println!("{} {}: {size} bytes in {:.1}s", artifact.name, artifact.version, start.elapsed().as_secs_f64());
    if size != artifact.size {
        return Err(format!("{size} bytes, pinned {}", artifact.size));
    }
    if sha256 != artifact.sha256 {
        return Err(format!("SHA-256 {sha256}, pinned {}", artifact.sha256));
    }
    Ok(())
}

#[test]
#[ignore = "downloads about 230 MB"]
fn every_pinned_helper_file_matches_its_pin() {
    let transport = HttpsTransport::new(env!("CARGO_PKG_VERSION"));
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();
    for arch in [Arch::Arm64, Arch::X64] {
        for helper in HELPERS {
            for artifact in helper.artifacts(arch) {
                if !seen.insert(artifact.url.clone()) {
                    continue;
                }
                if let Err(why) = check(&transport, &artifact) {
                    failures.push(format!("{}: {why}", artifact.url));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(seen.len(), 25, "two Pythons, two Buns, TinyTeX and 20 TeX Live packages");
}
