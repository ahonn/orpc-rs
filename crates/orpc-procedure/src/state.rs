use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A no-op hasher optimized for `TypeId` keys.
///
/// `TypeId` values are already well-distributed hashes, so re-hashing
/// them is wasted work. This hasher simply passes through the `u64`.
#[derive(Default)]
struct NoOpHasher(u64);

impl Hasher for NoOpHasher {
    fn write(&mut self, _bytes: &[u8]) {
        unimplemented!("NoOpHasher only supports u64 (TypeId)")
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type TypeMap = HashMap<TypeId, Box<dyn Any + Send + Sync>, BuildHasherDefault<NoOpHasher>>;

/// Type-safe heterogeneous state container.
///
/// Stores values keyed by their `TypeId`, allowing type-safe insertion and retrieval.
/// Used for dependency injection and cross-procedure shared state.
///
/// Follows rspc's `State` pattern with `NoOpHasher` optimization.
pub struct State(TypeMap);

impl State {
    pub fn new() -> Self {
        State(HashMap::default())
    }

    /// Insert a value. Replaces any existing value of the same type.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a reference to a stored value by type.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>()).and_then(|v| v.downcast_ref())
    }

    /// Get a mutable reference to a stored value by type.
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.0
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut())
    }

    /// Check if a value of the given type exists.
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.0.contains_key(&TypeId::of::<T>())
    }

    /// Remove and return a stored value by type.
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.0
            .remove(&TypeId::of::<T>())
            .and_then(|v| v.downcast().ok())
            .map(|v| *v)
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("State")
            .field(&format!("{} entries", self.0.len()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut state = State::new();
        state.insert(42u32);
        assert_eq!(state.get::<u32>(), Some(&42));
    }

    #[test]
    fn insert_replaces_existing() {
        let mut state = State::new();
        state.insert(1u32);
        state.insert(2u32);
        assert_eq!(state.get::<u32>(), Some(&2));
    }

    #[test]
    fn get_missing_returns_none() {
        let state = State::new();
        assert_eq!(state.get::<u32>(), None);
    }

    #[test]
    fn get_mut_allows_modification() {
        let mut state = State::new();
        state.insert(String::from("hello"));
        if let Some(s) = state.get_mut::<String>() {
            s.push_str(" world");
        }
        assert_eq!(state.get::<String>().unwrap(), "hello world");
    }

    #[test]
    fn contains() {
        let mut state = State::new();
        assert!(!state.contains::<u32>());
        state.insert(42u32);
        assert!(state.contains::<u32>());
    }

    #[test]
    fn remove_returns_owned_value() {
        let mut state = State::new();
        state.insert(String::from("removed"));
        let removed = state.remove::<String>();
        assert_eq!(removed, Some(String::from("removed")));
        assert!(!state.contains::<String>());
    }

    #[test]
    fn multiple_types() {
        let mut state = State::new();
        state.insert(42u32);
        state.insert("hello");
        state.insert(vec![1, 2, 3]);

        assert_eq!(state.get::<u32>(), Some(&42));
        assert_eq!(state.get::<&str>(), Some(&"hello"));
        assert_eq!(state.get::<Vec<i32>>(), Some(&vec![1, 2, 3]));
    }

    #[test]
    fn debug_output() {
        let mut state = State::new();
        state.insert(1u32);
        state.insert("hello");
        let debug = format!("{state:?}");
        assert!(debug.contains("2 entries"));
    }

    #[test]
    fn state_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<State>();
    }
}
