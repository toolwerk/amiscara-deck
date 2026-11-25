//! Which build a program is: a fingerprint of its file. After an update the app
//! compares the daemon that still runs with the one it brings.

use std::io::Read;
use std::path::Path;

/// FNV-1a over the content of the file, as 16 hex digits. Not for security:
/// it only tells two builds apart. `None` if the file cannot be read.
pub fn fingerprint(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut block = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block).ok()?;
        if n == 0 {
            return Some(format!("{hash:016x}"));
        }
        for byte in &block[..n] {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_content_gives_the_same_fingerprint() {
        let dir = std::env::temp_dir().join(format!("deck-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, b"amiscara").expect("write");
        std::fs::write(&b, b"amiscara").expect("write");
        assert_eq!(fingerprint(&a), fingerprint(&b));
        std::fs::write(&b, b"amiscarb").expect("write");
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&dir.join("none")), None);
        assert_eq!(fingerprint(&a).map(|f| f.len()), Some(16));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
