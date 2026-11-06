use crate::sync::remote::{Content, FileContent};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io;
use std::path::Path;

const BLOCK: usize = 128 * 1024;

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn hash_content(content: &dyn Content) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut at = 0usize;
    while at < content.byte_len() {
        let end = (at + BLOCK).min(content.byte_len());
        content.read(at..end, &mut |bytes| {
            hasher.update(bytes);
            Ok(())
        })?;
        at = end;
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn hash_file(path: &Path) -> io::Result<String> {
    hash_content(&FileContent::open(path)?)
}

/// Dropbox's content hash: SHA-256 of each 4 MiB block, then SHA-256 of the
/// concatenated block digests. This is the only hash Dropbox accepts for
/// comparing local content with remote metadata.
pub fn dropbox_hash(content: &dyn Content) -> io::Result<String> {
    const HASH_BLOCK: usize = 4 * 1024 * 1024;
    let mut whole = Sha256::new();
    let mut at = 0usize;
    while at < content.byte_len() {
        let end = (at + HASH_BLOCK).min(content.byte_len());
        let mut block = Sha256::new();
        content.read(at..end, &mut |bytes| {
            block.update(bytes);
            Ok(())
        })?;
        whole.update(block.finalize());
        at = end;
    }
    let mut hex = String::with_capacity(64);
    for byte in whole.finalize() {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropbox_hash_of_empty_content_matches_the_known_value() {
        let empty: Vec<u8> = Vec::new();
        assert_eq!(
            dropbox_hash(&empty).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn dropbox_hash_uses_the_block_scheme() {
        let data: Vec<u8> = (0..9_000_003u32).map(|index| (index % 251) as u8).collect();
        assert_eq!(
            dropbox_hash(&data).unwrap(),
            "3059bcac3d0b8a504296934f7b4cc3b6d88cb1e64b003970498407e2ffc4eba2"
        );
    }

    #[test]
    fn local_hash_is_plain_sha256() {
        assert_eq!(hash_bytes(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
