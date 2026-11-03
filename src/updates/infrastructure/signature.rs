use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

/// The public half of the key that signs each release, as `cargo packager signer generate` writes it.
/// The committed file is a placeholder until the real key replaces it, and a build that carries it cannot update.
const EMBEDDED: &str = include_str!("../../../assets/update/jobdesk-update.pub");

/// The key a download must be signed with.
#[derive(Clone)]
pub struct UpdateKey(PublicKey);

impl UpdateKey {
    /// The key built into this copy. `None` while it is still the placeholder.
    pub fn embedded() -> Option<UpdateKey> {
        Self::decode(EMBEDDED)
    }

    /// Reads a key in cargo packager's form: a minisign public key file, written out in base64.
    pub fn decode(text: &str) -> Option<UpdateKey> {
        let file = String::from_utf8(STANDARD.decode(text.trim()).ok()?).ok()?;
        PublicKey::decode(&file).ok().map(UpdateKey)
    }
}

/// Whether the file at `path` is the one `signature` signs with `key`. `signature` is in cargo packager's form:
/// a minisign signature file in base64, as the feed carries it. A signature that cannot be read is a mismatch.
/// The file is read in pieces, so a large one never sits in memory whole.
pub fn signed_by(key: &UpdateKey, path: &Path, signature: &str) -> io::Result<bool> {
    let Some(signature) = decode_signature(signature) else { return Ok(false) };
    let Ok(mut verifier) = key.0.verify_stream(&signature) else { return Ok(false) };
    let mut file = File::open(path)?;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    Ok(verifier.finalize().is_ok())
}

fn decode_signature(signature: &str) -> Option<Signature> {
    let file = String::from_utf8(STANDARD.decode(signature.trim()).ok()?).ok()?;
    Signature::decode(&file).ok()
}

#[cfg(test)]
pub mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// The public key of a throwaway pair made only to sign the fixtures. Its secret half was deleted once they were signed.
    /// `tests/fixtures/updates` holds two stand-in builds signed with `cargo packager signer sign`, and the feed
    /// `scripts/update-feed.sh` wrote for them with `PUB_DATE=2026-10-30T09:00:00Z`. `other.pub` is a second key.
    pub fn fixture_key() -> UpdateKey {
        UpdateKey::decode(&fs::read_to_string(fixture("throwaway.pub")).unwrap()).unwrap()
    }

    pub fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/updates").join(name)
    }

    pub fn fixture_signature(name: &str) -> String {
        fs::read_to_string(fixture(&format!("{name}.sig"))).unwrap()
    }

    const MAC: &str = "Jobdesk-v0.2.0-macos-arm64.app.tar.gz";
    const WINDOWS: &str = "Jobdesk-v0.2.0-windows-x64-setup.exe";

    #[test]
    fn the_placeholder_key_is_no_key_and_a_real_one_reads() {
        assert_eq!(UpdateKey::embedded().is_none(), EMBEDDED.starts_with("PLACEHOLDER"), "the built-in key is the placeholder or a key that reads");
        assert!(UpdateKey::decode("PLACEHOLDER: this build has no update key.").is_none());
        assert!(UpdateKey::decode("").is_none());
        assert!(UpdateKey::decode("not base64 at all").is_none());
    }

    #[test]
    fn a_file_signed_with_the_key_passes() {
        let key = fixture_key();
        assert!(signed_by(&key, &fixture(MAC), &fixture_signature(MAC)).unwrap());
        assert!(signed_by(&key, &fixture(WINDOWS), &fixture_signature(WINDOWS)).unwrap());
    }

    #[test]
    fn a_flipped_byte_or_another_file_fails() {
        let key = fixture_key();
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = fs::read(fixture(WINDOWS)).unwrap();
        bytes[0] ^= 1;
        let flipped = dir.path().join(WINDOWS);
        fs::write(&flipped, bytes).unwrap();
        assert!(!signed_by(&key, &flipped, &fixture_signature(WINDOWS)).unwrap());
        assert!(!signed_by(&key, &fixture(MAC), &fixture_signature(WINDOWS)).unwrap(), "a signature fits only its own file");
    }

    #[test]
    fn a_file_signed_with_another_key_fails() {
        let other = UpdateKey::decode(&fs::read_to_string(fixture("other.pub")).unwrap()).unwrap();
        assert!(!signed_by(&other, &fixture(WINDOWS), &fixture_signature(WINDOWS)).unwrap());
    }

    #[test]
    fn a_signature_that_cannot_be_read_fails() {
        let key = fixture_key();
        for bad in ["", "sig", "dW50cnVzdGVkIGNvbW1lbnQ6IG5vdGhpbmcK"] {
            assert!(!signed_by(&key, &fixture(WINDOWS), bad).unwrap(), "{bad:?}");
        }
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_pass() {
        assert!(signed_by(&fixture_key(), Path::new("/no/such/file"), &fixture_signature(WINDOWS)).is_err());
    }
}
