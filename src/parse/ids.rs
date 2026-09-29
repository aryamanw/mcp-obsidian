use std::hash::{BuildHasher, Hasher};

/// Returns a random id of `len` characters drawn from `alphabet`. Uses std's
/// per-instance randomly keyed `RandomState` as the entropy source — these
/// ids only need to be unique within one canvas/drawing, not cryptographic.
pub fn random_id(len: usize, alphabet: &[u8]) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut bits = std::collections::hash_map::RandomState::new().build_hasher().finish();
        for _ in 0..8 {
            if out.len() == len {
                break;
            }
            out.push(alphabet[(bits % alphabet.len() as u64) as usize] as char);
            bits /= alphabet.len() as u64;
        }
    }
    out
}

pub const HEX: &[u8] = b"0123456789abcdef";
pub const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// A random 32-bit integer, for Excalidraw's `seed`/`versionNonce` fields.
pub fn random_u32() -> u32 {
    std::collections::hash_map::RandomState::new().build_hasher().finish() as u32
}
