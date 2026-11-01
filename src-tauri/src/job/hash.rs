//! SHA-256 over files and byte slices, used by the publish step and the undo
//! journal (`ARCHITECTURE.md` §9.4, §13).
//!
//! Why a hash at all: undo must be able to prove that a file at a recorded path
//! is still *the file this job wrote* before deleting it. A path alone is not
//! proof — the user may have replaced it, a symlink may now point somewhere
//! else, or the drive may have been rebuilt. The digest is that proof.
//!
//! `sha2` (RustCrypto, MIT OR Apache-2.0) is used rather than a hand-rolled
//! implementation; it is already in the dependency graph (and therefore in
//! `docs/LICENSES.md`), and rolling our own digest for a delete guard would be a
//! worse liability than a dependency.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// Streaming read size. 64 KiB keeps a 4 GB video from being buffered.
const CHUNK: usize = 64 * 1024;

/// Lowercase hex SHA-256 of a byte slice.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// Lowercase hex SHA-256 of a file's contents.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(digest: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn nist_vectors() {
        // FIPS 180-2 appendix B / the well-known empty-string digest. If these
        // do not hold, the delete guard in the journal is worthless.
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn file_digest_matches_the_byte_digest_across_a_chunk_boundary() {
        let dir = std::env::temp_dir().join(format!("ezc-hash-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // Bigger than CHUNK on purpose: the streaming loop is the code under test.
        let payload: Vec<u8> = (0..(CHUNK * 2 + 17)).map(|i| (i % 251) as u8).collect();
        let path = dir.join("payload.bin");
        fs::write(&path, &payload).unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(&payload));

        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
