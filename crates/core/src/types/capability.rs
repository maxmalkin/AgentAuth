//! Capability types for AgentAuth.
//!
//! Capabilities are hierarchical and define what actions an agent can perform.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A capability that an agent can request or be granted.
///
/// Capabilities are hierarchical:
/// - `Read` - Read-only access to a resource
/// - `Write` - Write access to a resource (implies Read)
/// - `Transact` - Financial or irreversible transactions (requires two-step approval)
/// - `Delete` - Deletion capability (requires two-step approval)
/// - `Custom` - Custom namespace-scoped capability
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Capability {
    /// Read-only access to a resource.
    Read {
        /// The resource being accessed (e.g., "calendar", "email", "files").
        resource: String,
        /// Optional filter to narrow the scope (e.g., "label:work").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<String>,
    },

    /// Write access to a resource.
    Write {
        /// The resource being modified.
        resource: String,
        /// Optional conditions for write access.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conditions: Option<WriteConditions>,
    },

    /// Financial or irreversible transaction capability.
    /// Requires two-step confirmation in the approval UI.
    Transact {
        /// The resource or system for transactions.
        resource: String,
        /// Maximum value per transaction (currency units depend on resource).
        max_value: u64,
        /// Currency or unit for the max_value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        currency: Option<String>,
    },

    /// Deletion capability.
    /// Requires two-step confirmation in the approval UI.
    Delete {
        /// The resource that can be deleted.
        resource: String,
        /// Optional filter to narrow deletion scope.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<String>,
    },

    /// Custom capability for extensibility.
    Custom {
        /// Namespace for the custom capability (e.g., "com.example").
        namespace: String,
        /// Name of the capability within the namespace.
        name: String,
        /// Additional parameters for the capability.
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        params: HashMap<String, serde_json::Value>,
    },
}

/// Conditions that restrict write access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteConditions {
    /// Only allow writes to items matching this filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,

    /// Maximum number of items that can be written per request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items_per_request: Option<u32>,

    /// Whether the write must be appending only (no overwrites).
    #[serde(default)]
    pub append_only: bool,
}

impl Capability {
    /// Validates the capability for internal consistency.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Resource names are empty
    /// - Transact has max_value of 0
    /// - Custom namespace or name are empty
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        match self {
            Capability::Read { resource, .. }
            | Capability::Write { resource, .. }
            | Capability::Delete { resource, .. } => {
                if resource.is_empty() {
                    return Err(crate::CoreError::InvalidCapability(
                        "resource cannot be empty".to_string(),
                    ));
                }
            }
            Capability::Transact {
                resource,
                max_value,
                ..
            } => {
                if resource.is_empty() {
                    return Err(crate::CoreError::InvalidCapability(
                        "resource cannot be empty".to_string(),
                    ));
                }
                if *max_value == 0 {
                    return Err(crate::CoreError::InvalidCapability(
                        "max_value must be greater than 0 for Transact capability".to_string(),
                    ));
                }
            }
            Capability::Custom {
                namespace, name, ..
            } => {
                if namespace.is_empty() {
                    return Err(crate::CoreError::InvalidCapability(
                        "namespace cannot be empty for Custom capability".to_string(),
                    ));
                }
                if name.is_empty() {
                    return Err(crate::CoreError::InvalidCapability(
                        "name cannot be empty for Custom capability".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Returns the resource associated with this capability.
    #[must_use]
    pub fn resource(&self) -> &str {
        match self {
            Capability::Read { resource, .. }
            | Capability::Write { resource, .. }
            | Capability::Transact { resource, .. }
            | Capability::Delete { resource, .. } => resource,
            Capability::Custom { namespace, .. } => {
                // Custom capabilities don't have a direct resource - return namespace
                namespace
            }
        }
    }

    /// Returns true if this capability requires two-step confirmation.
    #[must_use]
    pub fn requires_two_step_confirmation(&self) -> bool {
        matches!(
            self,
            Capability::Transact { .. } | Capability::Delete { .. }
        )
    }

    /// Returns the capability type as a string.
    #[must_use]
    pub fn capability_type(&self) -> &'static str {
        match self {
            Capability::Read { .. } => "read",
            Capability::Write { .. } => "write",
            Capability::Transact { .. } => "transact",
            Capability::Delete { .. } => "delete",
            Capability::Custom { .. } => "custom",
        }
    }

    /// Converts the capability to a human-readable description.
    #[must_use]
    pub fn to_human_readable(&self) -> String {
        use std::fmt::Write;
        match self {
            Capability::Read { resource, filter } => match filter {
                Some(f) => format!("Read access to {resource} (filtered: {f})"),
                None => format!("Read access to {resource}"),
            },
            Capability::Write {
                resource,
                conditions,
            } => {
                let mut desc = format!("Write access to {resource}");
                if let Some(cond) = conditions {
                    if cond.append_only {
                        desc.push_str(" (append only)");
                    }
                    if let Some(max) = cond.max_items_per_request {
                        let _ = write!(desc, " (max {max} items per request)");
                    }
                }
                desc
            }
            Capability::Transact {
                resource,
                max_value,
                currency,
            } => {
                let curr = currency.as_deref().unwrap_or("units");
                format!("Transact on {resource} (max {max_value} {curr} per transaction)")
            }
            Capability::Delete { resource, filter } => match filter {
                Some(f) => format!("Delete from {resource} (filtered: {f})"),
                None => format!("Delete from {resource}"),
            },
            Capability::Custom {
                namespace, name, ..
            } => {
                format!("Custom capability: {namespace}:{name}")
            }
        }
    }
}

/// Computes a hash of a capability set for idempotency checks.
///
/// The hash is computed over the canonical JSON representation of the sorted capabilities.
pub fn hash_capability_set(capabilities: &[Capability]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    // Sort capabilities by their JSON representation for determinism
    let mut cap_strings: Vec<String> = capabilities
        .iter()
        .filter_map(|c| serde_json::to_string(c).ok())
        .collect();
    cap_strings.sort();

    let combined = cap_strings.join(",");
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_validation_empty_resource() {
        let cap = Capability::Read {
            resource: String::new(),
            filter: None,
        };
        assert!(cap.validate().is_err());
    }

    #[test]
    fn test_capability_validation_zero_max_value() {
        let cap = Capability::Transact {
            resource: "payments".to_string(),
            max_value: 0,
            currency: None,
        };
        assert!(cap.validate().is_err());
    }

    #[test]
    fn test_capability_validation_empty_namespace() {
        let cap = Capability::Custom {
            namespace: String::new(),
            name: "test".to_string(),
            params: HashMap::new(),
        };
        assert!(cap.validate().is_err());
    }

    #[test]
    fn test_capability_requires_two_step() {
        assert!(Capability::Transact {
            resource: "payments".to_string(),
            max_value: 100,
            currency: None,
        }
        .requires_two_step_confirmation());

        assert!(Capability::Delete {
            resource: "files".to_string(),
            filter: None,
        }
        .requires_two_step_confirmation());

        assert!(!Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        }
        .requires_two_step_confirmation());
    }

    #[test]
    fn test_capability_to_human_readable() {
        let cap = Capability::Read {
            resource: "calendar".to_string(),
            filter: Some("label:work".to_string()),
        };
        let readable = cap.to_human_readable();
        assert!(readable.contains("calendar"));
        assert!(readable.contains("label:work"));
    }

    #[test]
    fn test_capability_hash_deterministic() {
        let caps = vec![
            Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            },
            Capability::Write {
                resource: "email".to_string(),
                conditions: None,
            },
        ];

        let hash1 = hash_capability_set(&caps);
        let hash2 = hash_capability_set(&caps);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_capability_hash_order_independent() {
        let caps1 = vec![
            Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            },
            Capability::Write {
                resource: "email".to_string(),
                conditions: None,
            },
        ];

        let caps2 = vec![
            Capability::Write {
                resource: "email".to_string(),
                conditions: None,
            },
            Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            },
        ];

        // Hashes should be the same regardless of order
        assert_eq!(hash_capability_set(&caps1), hash_capability_set(&caps2));
    }

    #[test]
    fn test_capability_serialization_roundtrip() {
        let cap = Capability::Transact {
            resource: "payments".to_string(),
            max_value: 1000,
            currency: Some("USD".to_string()),
        };

        let json = serde_json::to_string(&cap).expect("serialize");
        let deserialized: Capability = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(cap, deserialized);
    }
}
