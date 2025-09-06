use alloc::collections::BTreeMap;

#[allow(unused)]
/// Extension trait for BTreeMap providing a take_entry method
pub trait BTreeMapExt<K, V> {
    /// Removes the entry for the given key and returns the value (if it existed)
    /// along with a vacant entry for reinsertion
    fn take_entry(&mut self, key: K) -> (Option<V>, btree_map::VacantEntry<'_, K, V>);
}

impl<K: Ord, V> BTreeMapExt<K, V> for BTreeMap<K, V> {
    fn take_entry(&mut self, key: K) -> (Option<V>, btree_map::VacantEntry<'_, K, V>) {
        let value = self.remove(&key);
        let entry = match self.entry(key) {
            btree_map::Entry::Vacant(v) => v,
            btree_map::Entry::Occupied(_) => unreachable!("we just removed this key"),
        };
        (value, entry)
    }
}

pub use alloc::collections::btree_map;

#[cfg(feature = "std")]
pub use std::collections::hash_map;

#[cfg(feature = "std")]
use std::collections::HashMap;

/// Extension trait for HashMap providing a take_entry method
#[cfg(feature = "std")]
pub trait HashMapExt<K, V> {
    /// Removes the entry for the given key and returns the value (if it existed)
    /// along with a vacant entry for reinsertion
    fn take_entry(&mut self, key: K) -> (Option<V>, hash_map::VacantEntry<'_, K, V>);
}

#[cfg(feature = "std")]
impl<K: Eq + std::hash::Hash, V> HashMapExt<K, V> for HashMap<K, V> {
    fn take_entry(&mut self, key: K) -> (Option<V>, hash_map::VacantEntry<'_, K, V>) {
        let value = self.remove(&key);
        let entry = match self.entry(key) {
            hash_map::Entry::Vacant(v) => v,
            hash_map::Entry::Occupied(_) => unreachable!("we just removed this key"),
        };
        (value, entry)
    }
}
