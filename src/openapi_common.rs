use serde::{Deserialize, Serialize};
use serde::de::{Deserializer, Error as DeError};
use serde::ser::Serializer;
use std::collections::HashMap;
use serde_json::Value;

pub fn serialize<S>(extensions: &HashMap<String, Value>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let filtered: HashMap<_, _> = extensions
        .iter()
        .filter(|(k, _)| k.starts_with("x-"))
        .collect();

    filtered.serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut map = HashMap::<String, Value>::deserialize(deserializer)?;

    // Keep only x- fields
    map.retain(|k, _| k.starts_with("x-"));

    Ok(map)
}