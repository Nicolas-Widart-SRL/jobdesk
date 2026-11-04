use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

/// The public half of the key that signs each release, as `cargo packager signer generate` writes it.
/// The committed file holds the release public key in base64.
const EMBEDDED: &str = include_str!("../../../assets/update/jobdesk-update.pub");

/// The key a download must be signed with.
#[derive(Clone)]
pub struct UpdateKey(PublicKey);

impl UpdateKey {
    /// The key built into this copy. `None` only when the built-in text is not a readable key.
    pub fn embedded() -> Option<UpdateKey> {
        Self::decode(EMBEDDED)
    }

    /// Reads a key in cargo packager's form: a minisign public key file, written out in base64.
    pub fn decode(text: &str) -> Option<UpdateKey> {
        let file = String::from_utf8(STANDARD.decode(text.trim()).ok()?).ok()?;
        PublicKey::decode(&file).ok().map(UpdateKey)
    }
}

/// Whether the file at `path` is the build `signature` signs with `key`, under the name `asset`.
/// `signature` is a minisign signature file in base64, as the feed carries it. Its signed trusted comment must name
/// `asset`, the versioned name of the build being checked, so a signature of another build or another version never
/// passes. A signature that cannot be read is a mismatch, and only a file that cannot be read is an error.
/// The file is read in pieces, so a large one never sits in memory whole.
pub fn signed_by(key: &UpdateKey, path: &Path, signature: &str, asset: &str) -> io::Result<bool> {
    let Some(signature) = decode_signature(signature, asset) else { return Ok(false) };
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

/// As `signed_by`, for bytes already read, so what is checked is exactly what is then used.
pub fn signs(key: &UpdateKey, bytes: &[u8], signature: &str, asset: &str) -> bool {
    decode_signature(signature, asset).is_some_and(|signature| key.0.verify(bytes, &signature, false).is_ok())
}

/// The signature, when it reads and its trusted comment names `asset` as its one file. The key checks the comment with
/// the rest of the signature, so it cannot be changed without failing.
fn decode_signature(signature: &str, asset: &str) -> Option<Signature> {
    let file = String::from_utf8(STANDARD.decode(signature.trim()).ok()?).ok()?;
    let signature = Signature::decode(&file).ok()?;
    let mut files = signature.trusted_comment().split('\t').filter_map(|part| part.strip_prefix("file:"));
    (files.next() == Some(asset) && files.next().is_none()).then_some(signature)
}

#[cfg(test)]
pub mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// The public key of a throwaway pair made only to sign the fixtures. Its secret half was deleted once they were signed.
    /// `tests/fixtures/updates` holds two stand-in builds signed under their release names with
    /// `cargo packager signer sign`, and the feed `scripts/update-feed.sh` wrote for them with
    /// `PUB_DATE=2026-10-30T09:00:00Z`. `Jobdesk-v0.1.9-windows-x64-setup.exe.sig` signs the same bytes as the 0.2.0
    /// setup, under the 0.1.9 name. `other.pub` is a second key.
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
    const OLDER: &str = "Jobdesk-v0.1.9-windows-x64-setup.exe";

    #[test]
    fn the_built_in_key_reads_and_text_that_is_no_key_does_not() {
        assert!(UpdateKey::embedded().is_some(), "the committed key reads");
        assert!(UpdateKey::decode("PLACEHOLDER: this build has no update key.").is_none());
        assert!(UpdateKey::decode("").is_none());
        assert!(UpdateKey::decode("not base64 at all").is_none());
    }

    #[test]
    fn a_file_signed_with_the_key_passes() {
        let key = fixture_key();
        assert!(signed_by(&key, &fixture(MAC), &fixture_signature(MAC), MAC).unwrap());
        assert!(signed_by(&key, &fixture(WINDOWS), &fixture_signature(WINDOWS), WINDOWS).unwrap());
        assert!(signs(&key, &fs::read(fixture(WINDOWS)).unwrap(), &fixture_signature(WINDOWS), WINDOWS));
    }

    #[test]
    fn a_valid_signature_of_another_name_or_version_fails() {
        let key = fixture_key();
        let bytes = fs::read(fixture(WINDOWS)).unwrap();
        assert!(signs(&key, &bytes, &fixture_signature(OLDER), OLDER), "the signature itself is good");
        assert!(!signs(&key, &bytes, &fixture_signature(OLDER), WINDOWS), "but it names the 0.1.9 build, not 0.2.0");
        assert!(!signed_by(&key, &fixture(WINDOWS), &fixture_signature(OLDER), WINDOWS).unwrap());
        assert!(!signed_by(&key, &fixture(WINDOWS), &fixture_signature(WINDOWS), "Jobdesk-v0.2.0-windows-x64-setup").unwrap());
        assert!(!signs(&key, &bytes, &fixture_signature(WINDOWS), ""));
        assert!(!signs(&key, &bytes, &fixture_signature(WINDOWS), MAC));
    }

    #[test]
    fn a_flipped_byte_or_another_file_fails() {
        let key = fixture_key();
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = fs::read(fixture(WINDOWS)).unwrap();
        bytes[0] ^= 1;
        let flipped = dir.path().join(WINDOWS);
        fs::write(&flipped, &bytes).unwrap();
        assert!(!signed_by(&key, &flipped, &fixture_signature(WINDOWS), WINDOWS).unwrap());
        assert!(!signs(&key, &bytes, &fixture_signature(WINDOWS), WINDOWS));
        assert!(!signed_by(&key, &fixture(MAC), &fixture_signature(WINDOWS), WINDOWS).unwrap(), "a signature fits only its own file");
    }

    #[test]
    fn a_file_signed_with_another_key_fails() {
        let other = UpdateKey::decode(&fs::read_to_string(fixture("other.pub")).unwrap()).unwrap();
        assert!(!signed_by(&other, &fixture(WINDOWS), &fixture_signature(WINDOWS), WINDOWS).unwrap());
    }

    #[test]
    fn a_signature_that_cannot_be_read_fails() {
        let key = fixture_key();
        for bad in ["", "sig", "dW50cnVzdGVkIGNvbW1lbnQ6IG5vdGhpbmcK"] {
            assert!(!signed_by(&key, &fixture(WINDOWS), bad, WINDOWS).unwrap(), "{bad:?}");
        }
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_pass() {
        assert!(signed_by(&fixture_key(), Path::new("/no/such/file"), &fixture_signature(WINDOWS), WINDOWS).is_err());
    }
}
