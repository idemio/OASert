use crate::error::ValidationErrorType;
use crate::validator::OpenApiPayloadValidator;
use dashmap::{DashMap, Entry, VacantEntry};
use serde_json::{Error, Value};
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::path::Path;
use std::sync::{Arc, OnceLock};

static GLOBAL_CACHE: OnceLock<ValidatorCollection<String>> = OnceLock::new();
pub fn global_validator_cache() -> &'static ValidatorCollection<String> {
    GLOBAL_CACHE.get_or_init(ValidatorCollection::new)
}

/// Error types for cache operations
#[derive(Debug)]
pub enum CacheError {
    /// The validator with the specified ID was not found in the cache
    ValidatorNotFound,
    /// The validator with the specified ID already exists in the cache
    ValidatorAlreadyExists,
    /// Attempted to create a new validator but failed.
    FailedToCreateValidator(ValidationErrorType),
}

impl Display for CacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::ValidatorNotFound => write!(f, "Validator not found in cache"),
            CacheError::ValidatorAlreadyExists => write!(f, "Validator already exists in cache"),
            CacheError::FailedToCreateValidator(err) => {
                write!(f, "Failed to create new validator: {}", err)
            }
        }
    }
}

impl std::error::Error for CacheError {}

pub struct ValidatorCollection<K> {
    cache: DashMap<K, Arc<OpenApiPayloadValidator>>,
}

impl<K> ValidatorCollection<K>
where
    K: Hash + Eq,
{
    pub fn new() -> Self {
        ValidatorCollection {
            cache: DashMap::new(),
        }
    }

    pub fn insert_from_file_path<P>(
        &self,
        id: K,
        file_path: P,
    ) -> Result<Arc<OpenApiPayloadValidator>, CacheError>
    where
        P: AsRef<Path>,
    {
        let path = file_path.as_ref();
        let content = match std::fs::read_to_string(path) {
            Ok(x) => x,
            Err(_) => todo!(),
        };
        let content: Value = match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(e) => panic!("Failed to parse JSON: {}", e),
        };
        self.insert(id, content)
    }

    pub fn insert<V>(&self, id: K, spec: V) -> Result<Arc<OpenApiPayloadValidator>, CacheError>
    where
        V: serde::Serialize,
    {
        match self.cache.entry(id) {
            Entry::Occupied(_) => Err(CacheError::ValidatorAlreadyExists),
            Entry::Vacant(entry) => Self::create_validator(entry, spec),
        }
    }

    fn create_validator<V>(
        entry: VacantEntry<K, Arc<OpenApiPayloadValidator>>,
        spec: V,
    ) -> Result<Arc<OpenApiPayloadValidator>, CacheError>
    where
        V: serde::Serialize,
    {
        let spec = match serde_json::to_value(spec) {
            Ok(val) => val,
            Err(_) => todo!(),
        };
        match OpenApiPayloadValidator::new(spec) {
            Ok(validator) => {
                let validator = Arc::new(validator);
                entry.insert(validator.clone());
                Ok(validator)
            }
            Err(e) => Err(CacheError::FailedToCreateValidator(e)),
        }
    }

    pub fn get(&self, id: &K) -> Result<Arc<OpenApiPayloadValidator>, CacheError> {
        match self.cache.get(id) {
            Some(validator) => Ok(Arc::clone(validator.value())),
            None => Err(CacheError::ValidatorNotFound),
        }
    }

    pub fn remove(&self, id: &K) -> Result<(), CacheError> {
        if self.cache.remove(id).is_none() {
            return Err(CacheError::ValidatorNotFound);
        }
        Ok(())
    }

    pub fn contains(&self, id: &K) -> bool {
        self.cache.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn clear(&self) {
        self.cache.clear();
        log::debug!("Cleared validator cache");
    }
}

#[cfg(test)]
mod tests {

    //    #[test]
    //    fn test_cache_get_insert() {
    //        let cache = ValidatorCache::new();
    //        assert!(cache.get("test").is_err());
    //        let spec = json!({
    //            "openapi": "3.1.0"
    //        });
    //        let validator = cache.insert("test".to_string(), spec).unwrap();
    //        assert!(!cache.is_empty());
    //        assert_eq!(cache.len(), 1);
    //        let cached = cache.get("test").unwrap();
    //        assert!(Arc::ptr_eq(&validator, &cached));
    //    }
    //
    //    #[test]
    //    fn test_cache_get_or_insert() {
    //        let cache = ValidatorCache::new();
    //        assert!(cache.get("test").is_err());
    //        let spec = json!({
    //            "openapi": "3.1.0"
    //        });
    //        let validator1 = cache
    //            .get_or_insert("test".to_string(), spec.clone())
    //            .unwrap();
    //        let validator2 = cache
    //            .get_or_insert("test".to_string(), json!({"openapi": "3.0.0"}))
    //            .unwrap();
    //        assert!(Arc::ptr_eq(&validator1, &validator2));
    //        assert_eq!(cache.len(), 1);
    //    }
    //
    //    #[test]
    //    fn test_cache_clear() {
    //        let cache = ValidatorCache::new();
    //        let spec = json!({
    //            "openapi": "3.1.0"
    //        });
    //        cache.insert("test1".to_string(), spec.clone()).unwrap();
    //        cache.insert("test2".to_string(), spec.clone()).unwrap();
    //        cache.insert("test3".to_string(), spec).unwrap();
    //        assert_eq!(cache.len(), 3);
    //        cache.clear();
    //        assert!(cache.is_empty());
    //    }
    //
    //    #[test]
    //    fn test_global_cache() {
    //        let cache = global_validator_cache();
    //        cache.clear();
    //        let spec = json!({
    //            "openapi": "3.1.0"
    //        });
    //        cache.insert("global_test".to_string(), spec).unwrap();
    //        let same_cache = global_validator_cache();
    //        assert!(same_cache.get("global_test").is_ok());
    //        cache.clear();
    //    }
}
