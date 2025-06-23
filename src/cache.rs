use crate::error::ValidationErrorType;
use crate::validator::OpenApiPayloadValidator;
use dashmap::{DashMap, VacantEntry};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock};

/// Global instance of the validator cache.
///
/// This provides a singleton instance that can be accessed from anywhere in the application.
// Global singleton instance of the validator cache
static GLOBAL_CACHE: OnceLock<ValidatorCache> = OnceLock::new();

/// Gets the global validator cache instance.
///
/// The cache is created on first access and reused for subsequent calls.
pub fn global_validator_cache() -> &'static ValidatorCache {
    GLOBAL_CACHE.get_or_init(ValidatorCache::new)
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

/// A global cache for OpenApiPayloadValidator instances.
///
/// This cache provides thread-safe storage and retrieval of validators
/// by their string identifiers using DashMap for concurrent access.
#[derive(Default)]
pub struct ValidatorCache {
    cache: DashMap<String, Arc<OpenApiPayloadValidator>>,
}

impl ValidatorCache {
    /// Creates a new empty validator cache
    pub fn new() -> Self {
        ValidatorCache {
            cache: DashMap::new(),
        }
    }

    /// Inserts a validator into the cache with the given ID.
    ///
    /// If a validator with the same ID already exists, returns an error.
    ///
    /// # Arguments
    /// * `id` - A string identifier for the validator
    /// * `validator` - The OpenApiPayloadValidator to store
    ///
    /// # Returns
    /// * `Ok(())` - If the validator was successfully inserted
    /// * `Err(CacheError)` - If a validator with the same ID already exists
    pub fn insert(
        &self,
        id: String,
        spec: Value,
    ) -> Result<Arc<OpenApiPayloadValidator>, CacheError> {
        match self.cache.entry(id) {
            dashmap::mapref::entry::Entry::Occupied(_) => Err(CacheError::ValidatorAlreadyExists),
            dashmap::mapref::entry::Entry::Vacant(entry) => Self::create_validator(entry, spec),
        }
    }

    fn create_validator(
        entry: VacantEntry<String, Arc<OpenApiPayloadValidator>>,
        spec: Value,
    ) -> Result<Arc<OpenApiPayloadValidator>, CacheError> {
        match OpenApiPayloadValidator::new(spec) {
            Ok(validator) => {
                log::debug!("Added validator to cache with ID: {}", entry.key());
                let validator = Arc::new(validator);
                entry.insert(validator.clone());
                Ok(validator)
            }
            Err(e) => {
                log::error!("Failed to create validator for ID {}: {}", entry.key(), e);
                Err(CacheError::FailedToCreateValidator(e))
            }
        }
    }

    /// Inserts or replaces a validator in the cache with the given ID.
    ///
    /// # Arguments
    /// * `id` - A string identifier for the validator
    /// * `validator` - The OpenApiPayloadValidator to store
    pub fn insert_or_replace(&self, id: String, validator: OpenApiPayloadValidator) {
        self.cache.insert(id, Arc::new(validator));
    }

    /// Retrieves a validator from the cache by its ID.
    ///
    /// # Arguments
    /// * `id` - The string identifier of the validator to retrieve
    ///
    /// # Returns
    /// * `Ok(Arc<OpenApiPayloadValidator>)` - A reference-counted pointer to the validator if found
    /// * `Err(CacheError)` - If no validator with the given ID exists in the cache
    pub fn get(&self, id: &str) -> Result<Arc<OpenApiPayloadValidator>, CacheError> {
        match self.cache.get(id) {
            Some(validator) => Ok(Arc::clone(validator.value())),
            None => Err(CacheError::ValidatorNotFound),
        }
    }

    /// Removes a validator from the cache by its ID.
    ///
    /// # Arguments
    /// * `id` - The string identifier of the validator to remove
    ///
    /// # Returns
    /// * `Ok(())` - If the validator was successfully removed
    /// * `Err(CacheError)` - If no validator with the given ID exists in the cache
    pub fn remove(&self, id: &str) -> Result<(), CacheError> {
        if self.cache.remove(id).is_none() {
            return Err(CacheError::ValidatorNotFound);
        }
        Ok(())
    }

    /// Gets a validator from the cache if it exists, or creates and caches a new one.
    ///
    /// # Arguments
    ///
    /// * `id` - The string identifier for the validator
    /// * `spec` - The OpenAPI specification as a JSON Value, used only if the validator isn't in the cache
    ///
    /// # Returns
    ///
    /// * `Result<Arc<OpenApiPayloadValidator>, ValidationError>` - The validator on success,
    ///   or a ValidationError if a new validator couldn't be created
    pub fn get_or_insert(
        &self,
        id: String,
        spec: Value,
    ) -> Result<Arc<OpenApiPayloadValidator>, CacheError> {
        match self.cache.entry(id) {
            dashmap::mapref::entry::Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
            dashmap::mapref::entry::Entry::Vacant(entry) => Self::create_validator(entry, spec),
        }
    }

    /// Checks if a validator with the given ID exists in the cache.
    ///
    /// # Arguments
    /// * `id` - The string identifier to check
    ///
    /// # Returns
    /// * `true` if a validator with the given ID exists in the cache, `false` otherwise
    pub fn contains(&self, id: &str) -> bool {
        self.cache.contains_key(id)
    }

    /// Returns the number of validators in the cache.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Checks if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clears all validators from the cache.
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
