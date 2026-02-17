//! Retained message store and sync
//!
//! Stores retained MQTT messages and provides cluster-wide sync.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

/// A stored retained message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetainedMessage {
    /// MQTT topic
    pub topic: String,
    /// Message payload (base64 encoded for JSON transport)
    #[serde(with = "base64_serde")]
    pub payload: Vec<u8>,
    /// Timestamp when message was received
    pub timestamp: u64,
    /// Source node name (where message was originally published)
    pub source_node: String,
}

/// Store for retained messages
#[derive(Clone)]
pub struct RetainedStore {
    /// Messages indexed by topic
    messages: Arc<DashMap<String, RetainedMessage>>,
    /// Local node name
    node_name: String,
}

impl RetainedStore {
    /// Create a new retained message store
    pub fn new(node_name: String) -> Self {
        Self {
            messages: Arc::new(DashMap::new()),
            node_name,
        }
    }

    /// Store a retained message
    pub fn store(&self, topic: String, payload: Vec<u8>) {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let msg = RetainedMessage {
            topic: topic.clone(),
            payload,
            timestamp,
            source_node: self.node_name.clone(),
        };

        self.messages.insert(topic, msg);
    }

    /// Store a retained message from another node (sync)
    pub fn store_synced(&self, msg: RetainedMessage) {
        // Only store if newer or doesn't exist
        self.messages
            .entry(msg.topic.clone())
            .and_modify(|existing| {
                if msg.timestamp > existing.timestamp {
                    *existing = msg.clone();
                }
            })
            .or_insert(msg);
    }

    /// Get a retained message by topic
    pub fn get(&self, topic: &str) -> Option<RetainedMessage> {
        self.messages.get(topic).map(|r| r.clone())
    }

    /// Get all retained messages matching a topic pattern
    /// Supports wildcards: + (single level) and # (multi-level)
    pub fn get_matching(&self, pattern: &str) -> Vec<RetainedMessage> {
        if pattern == "#" {
            return self.get_all();
        }

        self.messages
            .iter()
            .filter(|r| topic_matches(pattern, &r.topic))
            .map(|r| r.clone())
            .collect()
    }

    /// Get all retained messages
    pub fn get_all(&self) -> Vec<RetainedMessage> {
        self.messages.iter().map(|r| r.clone()).collect()
    }

    /// Get count of stored messages
    pub fn count(&self) -> usize {
        self.messages.len()
    }

    /// Remove a retained message
    pub fn remove(&self, topic: &str) {
        self.messages.remove(topic);
    }

    /// Clear all retained messages
    pub fn clear(&self) {
        self.messages.clear();
    }
}

/// Check if a topic matches a pattern with wildcards
fn topic_matches(pattern: &str, topic: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let topic_parts: Vec<&str> = topic.split('/').collect();

    let mut pi = 0;
    let mut ti = 0;

    while pi < pattern_parts.len() && ti < topic_parts.len() {
        let pat = pattern_parts[pi];

        if pat == "#" {
            // Multi-level wildcard matches everything from here
            return true;
        } else if pat == "+" {
            // Single-level wildcard matches this level
            pi += 1;
            ti += 1;
        } else if pat == topic_parts[ti] {
            // Exact match
            pi += 1;
            ti += 1;
        } else {
            return false;
        }
    }

    // Both must be exhausted for a match (unless pattern ended with #)
    pi == pattern_parts.len() && ti == topic_parts.len()
}

/// Base64 serialization for payload
mod base64_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD
            .decode(&s)
            .map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_matches() {
        // Exact match
        assert!(topic_matches("foo/bar", "foo/bar"));
        assert!(!topic_matches("foo/bar", "foo/baz"));

        // Single-level wildcard
        assert!(topic_matches("foo/+/baz", "foo/bar/baz"));
        assert!(!topic_matches("foo/+/baz", "foo/bar/qux"));

        // Multi-level wildcard
        assert!(topic_matches("foo/#", "foo/bar"));
        assert!(topic_matches("foo/#", "foo/bar/baz"));
        assert!(topic_matches("#", "anything/goes"));
    }

    #[test]
    fn test_store_and_get() {
        let store = RetainedStore::new("test-node".to_string());

        store.store("test/topic".to_string(), b"hello".to_vec());

        let msg = store.get("test/topic").unwrap();
        assert_eq!(msg.topic, "test/topic");
        assert_eq!(msg.payload, b"hello");
        assert_eq!(msg.source_node, "test-node");
    }
}
