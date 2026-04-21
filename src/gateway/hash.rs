use sha2::{Digest, Sha256};

/// Used to generate a simple hash string from an input byte slice
pub fn hash_now(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let input = b"hello, world";
        assert_eq!(hash_now(input), hash_now(input));
    }

    #[test]
    fn hash_known_sha256_vector() {
        // Verified: echo -n "abc" | shasum -a 256
        assert_eq!(
            hash_now(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn different_inputs_produce_different_hashes() {
        assert_ne!(hash_now(b"address-a"), hash_now(b"address-b"));
    }

    #[test]
    fn empty_input_has_known_hash() {
        // Verified: echo -n "" | shasum -a 256
        assert_eq!(
            hash_now(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_output_is_64_hex_chars() {
        let h = hash_now(b"any input");
        assert_eq!(h.len(), 64, "SHA-256 hex output must be exactly 64 characters");
    }
}
