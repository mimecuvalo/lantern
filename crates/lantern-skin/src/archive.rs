//! Hardened extraction of untrusted `.wsz` archives.
//!
//! These files come from the open internet, so the extractor is written to
//! make the two classic ZIP attacks structurally impossible rather than
//! merely defended against:
//!
//! * **Zip slip.** Nothing is ever written to disk. Entries land in a map
//!   keyed by *lowercased basename* with every path component stripped,
//!   including backslash-separated Windows paths. A malicious
//!   `../../../.ssh/authorized_keys` entry simply becomes the key
//!   `authorized_keys`.
//! * **Zip bombs.** Capped on both sides: the declared `uncompressed_size`
//!   *and* a running count of bytes actually streamed, so a lying header
//!   buys an attacker nothing.
//!
//! Ported from the Swift implementation, which was audited and found sound.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use crate::SkinError;

/// Largest single entry we will hold. Real skins are far below this.
pub const PER_ENTRY_CAP: usize = 32 * 1024 * 1024;

/// Largest total across all entries.
pub const TOTAL_CAP: usize = 96 * 1024 * 1024;

/// Extract every file entry into `{lowercased basename: bytes}`.
///
/// Damaged entries are skipped rather than aborting the archive; validation of
/// what actually mattered happens later, in [`crate::parse`]. These files are
/// decades old and a surprising number have a bad CRC somewhere harmless.
pub fn extract(zip: &[u8]) -> Result<HashMap<String, Vec<u8>>, SkinError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(zip)).map_err(|_| SkinError::InvalidArchive)?;

    let mut out = HashMap::new();
    let mut total: usize = 0;

    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        if !entry.is_file() {
            continue;
        }
        // First cap: the declared size, so an honest bomb is rejected before
        // a single byte is decompressed.
        if entry.size() > PER_ENTRY_CAP as u64 {
            continue;
        }
        let name = basename(entry.name());
        if name.is_empty() {
            continue;
        }

        // Second cap: what actually arrives, so a lying header buys nothing.
        let mut bytes = Vec::new();
        if Read::by_ref(&mut entry)
            .take(PER_ENTRY_CAP as u64 + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        if bytes.len() > PER_ENTRY_CAP {
            continue;
        }

        total = total.saturating_add(bytes.len());
        if total > TOTAL_CAP {
            break;
        }
        out.insert(name, bytes);
    }

    Ok(out)
}

/// Reduce an archive path to the lowercased filename, discarding directories.
///
/// Handles both separators: skins built on Windows often carry `\`.
pub fn basename(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::basename;

    #[test]
    fn strips_directories_and_lowercases() {
        assert_eq!(basename("Skin/Main.BMP"), "main.bmp");
        assert_eq!(basename("main.bmp"), "main.bmp");
    }

    #[test]
    fn handles_windows_separators() {
        assert_eq!(basename(r"nested\dir\CBUTTONS.BMP"), "cbuttons.bmp");
    }

    #[test]
    fn defuses_traversal() {
        assert_eq!(basename("../../../.ssh/authorized_keys"), "authorized_keys");
        assert_eq!(basename(r"..\..\windows\system32\evil.dll"), "evil.dll");
    }
}
