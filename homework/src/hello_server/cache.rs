//! Thread-safe key/value cache.

use std::collections::hash_map::{Entry, HashMap};
use std::hash::Hash;
use std::sync::{Arc, Mutex, RwLock};

/// Cache that remembers the result for each key.
#[derive(Debug, Default)]
pub struct Cache<K, V> {
    // specification for `get_or_insert_with`.
    inner: Mutex<HashMap<K, Arc<Mutex<V>>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> {
    /// Retrieve the value or insert a new one created by `f`.
    ///
    /// An invocation to this function should not block another invocation with a different key. For
    /// example, if a thread calls `get_or_insert_with(key1, f1)` and another thread calls
    /// `get_or_insert_with(key2, f2)` (`key1≠key2`, `key1,key2∉cache`) concurrently, `f1` and `f2`
    /// should run concurrently.
    ///
    /// On the other hand, since `f` may consume a lot of resource (= money), it's desirable not to
    /// duplicate the work. That is, `f` should be run only once for each key. Specifically, even
    /// for the concurrent invocations of `get_or_insert_with(key, f)`, `f` is called only once.
    ///
    /// Hint: the [`Entry`] API may be useful in implementing this function.
    ///
    /// [`Entry`]: https://doc.rust-lang.org/stable/std/collections/hash_map/struct.HashMap.html#method.entry
    pub fn get_or_insert_with<F: FnOnce(K) -> V>(&self, key: K, f: F) -> V {
        let mut guard = self.inner.lock().unwrap();

        if let Some(arc_mutex_value) = guard.get(&key) {
            // Clone the data inside the Arc<Mutex<V>>
            return arc_mutex_value.lock().unwrap().clone();
        }

        // Temporarily drop the lock to allow other threads to access the cache
        drop(guard);

        // Compute the new value outside of the lock
        let new_value = f(key.clone());

        // Reacquire the lock to insert the new value
        let mut guard = self.inner.lock().unwrap();

        // Check again if the key was inserted by another thread while we were computing the value
        match guard.entry(key) {
            Entry::Occupied(entry) => entry.get().lock().unwrap().clone(),
            Entry::Vacant(entry) => {
                let arc_new_value = Arc::new(Mutex::new(new_value.clone()));
                entry.insert(arc_new_value);
                new_value
            },
        }
    }
}
