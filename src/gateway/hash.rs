use sha2::{Digest, Sha256};

/// Used to generate a simple hash string from an input byte slice
pub fn hash_now(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}
