use sha2::{Digest, Sha256};

/// Used to generate a simple hash string from an input string
pub fn hash_now(data: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use crate::gateway::hash::hash_now;

    #[test]
    fn valid_hash() {
        assert_eq!(
            hash_now("banana".to_string()),
            "b493d48364afe44d11c0165cf470a4164d1e2609911ef998be868d46ade3de4e"
        );
        assert_ne!(
            hash_now("asd".to_string()),
            "b493d48364afe44d11c0165cf470a4164d1e2609911ef998be868d46ade3de4e"
        );
    }
}
