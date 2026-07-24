//! Definition identifiers and the duplicate-key-rejecting definition map.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::NodeId;

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(NodeId);

        impl $name {
            /// Wraps a node ID as this definition identifier.
            #[must_use]
            pub const fn new(id: NodeId) -> Self {
                Self(id)
            }

            /// Returns the underlying node ID.
            #[must_use]
            pub const fn node_id(self) -> NodeId {
                self.0
            }
        }
    };
}

id_newtype!(
    /// Stable identity of a style definition.
    StyleId
);
id_newtype!(
    /// Stable identity of an abstract numbering definition.
    AbstractNumberingId
);
id_newtype!(
    /// Stable identity of a numbering instance.
    NumberingInstanceId
);
id_newtype!(
    /// Stable identity of a media reference.
    MediaId
);
id_newtype!(
    /// Stable identity of a section boundary.
    SectionId
);

/// A duplicate-key-rejecting, deterministically-ordered id map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionMap<K: Ord, V>(BTreeMap<K, V>);

impl<K: Ord, V> Default for DefinitionMap<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord, V> DefinitionMap<K, V> {
    /// Inserts an entry, returning any previous value for the key.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key, value)
    }

    /// Returns the value for a key, if present.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key)
    }

    /// Returns whether a key is present.
    pub fn contains_key(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates entries in ascending key order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }
}

impl<K: Ord + Serialize, V: Serialize> Serialize for DefinitionMap<K, V> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for DefinitionMap<K, V>
where
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapVisitor<K, V>(std::marker::PhantomData<(K, V)>);

        impl<'de, K, V> Visitor<'de> for MapVisitor<K, V>
        where
            K: Ord + Deserialize<'de>,
            V: Deserialize<'de>,
        {
            type Value = DefinitionMap<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object with unique definition keys")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = access.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate definition key"));
                    }
                }
                Ok(DefinitionMap(values))
            }
        }

        deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
    }
}
