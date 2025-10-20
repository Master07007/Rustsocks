use dashmap::{DashMap, mapref::one::RefMut};
use std::{
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

pub struct ExpiryMap<K, V> {
    map: DashMap<K, (Instant, V)>,
    expiry_duration: Duration,
}

pub struct ExpiryRefMut<'a, K, V> {
    entry_ref: RefMut<'a, K, (Instant, V)>,
}

impl<K, V> Deref for ExpiryRefMut<'_, K, V>
where
    K: std::hash::Hash + Eq,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.entry_ref.value().1
    }
}

impl<K, V> DerefMut for ExpiryRefMut<'_, K, V>
where
    K: std::hash::Hash + Eq,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry_ref.value_mut().1
    }
}

impl<K, V> ExpiryMap<K, V>
where
    K: std::hash::Hash + Eq,
{
    pub fn new(expiry_duration: Duration) -> Self {
        ExpiryMap {
            map: DashMap::new(),
            expiry_duration,
        }
    }

    pub fn insert(&self, key: K, value: V) {
        let now = Instant::now();
        self.map.insert(key, (now, value));
    }

    pub fn get_mut<'a>(&'a self, key: &K) -> Option<ExpiryRefMut<'a, K, V>> {
        if let Some(mut entry) = self.map.get_mut(key) {
            let (ins, _) = entry.value_mut();
            // update the timestamp
            *ins = Instant::now();
            let ref_mut = ExpiryRefMut { entry_ref: entry };
            return Some(ref_mut);
        }
        None
    }

    pub fn remove(&self, key: &K) {
        self.map.remove(key);
    }

    pub fn cleanup_expired(&self) {
        let now = Instant::now();
        for entry in self.map.iter() {
            let (ins, _) = entry.value();
            if now.duration_since(*ins) > self.expiry_duration {
                self.map.remove(entry.key());
            }
        }
    }
}
