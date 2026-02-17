//! API key validation using SHA256 hashes and HashSet for O(1) lookup

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

/// API key validator with O(1) lookup using pre-hashed keys
#[derive(Debug, Clone)]
pub struct ApiKeyValidator {
    /// Set of valid API key hashes (SHA256)
    valid_hashes: Arc<HashSet<String>>,
    /// Whether authentication is required
    require_auth: bool,
}

impl ApiKeyValidator {
    /// Create a new validator from a list of plaintext API keys
    pub fn new(api_keys: &[String], require_auth: bool) -> Self {
        let valid_hashes: HashSet<String> =
            api_keys.iter().map(|key| Self::hash_key(key)).collect();

        tracing::info!(
            "Initialized API key validator with {} keys, auth_required={}",
            valid_hashes.len(),
            require_auth
        );

        Self {
            valid_hashes: Arc::new(valid_hashes),
            require_auth,
        }
    }

    /// Create a validator that accepts all connections (no auth)
    pub fn allow_all() -> Self {
        Self {
            valid_hashes: Arc::new(HashSet::new()),
            require_auth: false,
        }
    }

    /// Hash an API key using SHA256
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Validate an API key
    ///
    /// Returns true if:
    /// - Authentication is not required, OR
    /// - The provided key's hash matches a valid hash
    pub fn validate(&self, api_key: &str) -> bool {
        if !self.require_auth {
            return true;
        }

        let hash = Self::hash_key(api_key);
        let valid = self.valid_hashes.contains(&hash);

        if !valid {
            tracing::debug!("Invalid API key attempt (hash prefix: {}...)", &hash[..8]);
        }

        valid
    }

    /// Validate an optional API key (for MQTT password field which can be None)
    pub fn validate_optional(&self, api_key: Option<&str>) -> bool {
        if !self.require_auth {
            return true;
        }

        match api_key {
            Some(key) => self.validate(key),
            None => {
                tracing::debug!("Authentication required but no API key provided");
                false
            }
        }
    }

    /// Get the number of registered API keys
    pub fn key_count(&self) -> usize {
        self.valid_hashes.len()
    }

    /// Check if authentication is required
    pub fn is_auth_required(&self) -> bool {
        self.require_auth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_correct_key() {
        let validator = ApiKeyValidator::new(&["secret-key".to_string()], true);
        assert!(validator.validate("secret-key"));
    }

    #[test]
    fn test_validate_incorrect_key() {
        let validator = ApiKeyValidator::new(&["secret-key".to_string()], true);
        assert!(!validator.validate("wrong-key"));
    }

    #[test]
    fn test_validate_multiple_keys() {
        let validator = ApiKeyValidator::new(
            &["key1".to_string(), "key2".to_string(), "key3".to_string()],
            true,
        );
        assert!(validator.validate("key1"));
        assert!(validator.validate("key2"));
        assert!(validator.validate("key3"));
        assert!(!validator.validate("key4"));
    }

    #[test]
    fn test_no_auth_required() {
        let validator = ApiKeyValidator::new(&[], false);
        assert!(validator.validate("anything"));
        assert!(validator.validate_optional(None));
    }

    #[test]
    fn test_auth_required_no_key() {
        let validator = ApiKeyValidator::new(&["key".to_string()], true);
        assert!(!validator.validate_optional(None));
    }

    #[test]
    fn test_allow_all() {
        let validator = ApiKeyValidator::allow_all();
        assert!(validator.validate("anything"));
        assert!(validator.validate_optional(None));
        assert!(!validator.is_auth_required());
    }

    #[test]
    fn test_hash_consistency() {
        // Same key should produce same hash
        let hash1 = ApiKeyValidator::hash_key("test-key");
        let hash2 = ApiKeyValidator::hash_key("test-key");
        assert_eq!(hash1, hash2);

        // Different keys should produce different hashes
        let hash3 = ApiKeyValidator::hash_key("other-key");
        assert_ne!(hash1, hash3);
    }
}
