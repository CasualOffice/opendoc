//! Stable node identity and deterministic ID generation.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ModelError;

/// Stable identity of a logical document node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u128);

impl NodeId {
    /// Creates a non-zero node ID.
    pub fn new(value: u128) -> Result<Self, ModelError> {
        if value == 0 {
            return Err(ModelError::ZeroNodeId);
        }
        Ok(Self(value))
    }

    /// Creates an ID from a namespace and a local counter.
    pub fn from_parts(namespace: u64, counter: u64) -> Result<Self, ModelError> {
        Self::new((u128::from(namespace) << 64) | u128::from(counter))
    }

    /// Returns the raw numeric representation.
    #[must_use]
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

impl FromStr for NodeId {
    type Err = ModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ModelError::InvalidNodeId);
        }

        let parsed = u128::from_str_radix(value, 16).map_err(|_| ModelError::InvalidNodeId)?;
        Self::new(parsed)
    }
}

impl Serialize for NodeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Deterministic namespace-and-counter ID source.
#[derive(Clone, Debug)]
pub struct IdGenerator {
    namespace: u64,
    next_counter: u64,
}

impl IdGenerator {
    /// Creates an ID generator whose first local counter is one.
    #[must_use]
    pub const fn new(namespace: u64) -> Self {
        Self {
            namespace,
            next_counter: 1,
        }
    }

    /// Returns the next ID or an error if the counter is exhausted.
    pub fn next_id(&mut self) -> Result<NodeId, ModelError> {
        if self.next_counter == u64::MAX {
            return Err(ModelError::IdSpaceExhausted);
        }

        let id = NodeId::from_parts(self.namespace, self.next_counter)?;
        self.next_counter += 1;
        Ok(id)
    }
}
