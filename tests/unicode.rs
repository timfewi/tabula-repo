#![forbid(unsafe_code)]
use sha2::{Digest, Sha256};
use tabula::compat::{casefold, decimal, is_printable, is_space, is_word};
#[test]
fn every_unicode_scalar_matches_frozen_python_315_unicode_17() {
    let mut hash = Sha256::new();
    for n in 0..=0x10ffff {
        if let Some(c) = char::from_u32(n) {
            let folded = casefold(&c.to_string());
            hash.update((folded.len() as u32).to_le_bytes());
            hash.update(folded.as_bytes());
            hash.update([
                is_word(c) as u8,
                is_printable(c) as u8,
                is_space(c) as u8,
                decimal(c).unwrap_or(255) as u8,
            ]);
        }
    }
    assert_eq!(
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        include_str!("reference/unicode.sha256").trim()
    );
}
