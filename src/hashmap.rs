//! Safe Rust reimplementation of Google V8's `src/base/hashmap.h`.
//!
//! Provides open-addressing hash table with linear probing and backward-shift
//! deletion (without tombstones), maintaining 100% algorithmic fidelity with V8.

/// Entry in the V8 hash table.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HashMapEntry<K, V> {
    pub key: K,
    pub value: V,
    hash_and_exists: u32,
}

impl<K: Default, V: Default> HashMapEntry<K, V> {
    pub const HASH_MASK: u32 = 0x7FFF_FFFF;
    pub const EXISTS_BIT: u32 = 0x8000_0000;

    pub fn empty() -> Self {
        Self {
            key: K::default(),
            value: V::default(),
            hash_and_exists: 0,
        }
    }

    pub fn new(key: K, value: V, hash: u32) -> Self {
        Self {
            key,
            value,
            hash_and_exists: (hash & Self::HASH_MASK) | Self::EXISTS_BIT,
        }
    }

    #[inline]
    pub fn exists(&self) -> bool {
        (self.hash_and_exists & Self::EXISTS_BIT) != 0
    }

    #[inline]
    pub fn hash(&self) -> u32 {
        self.hash_and_exists & Self::HASH_MASK
    }

    #[inline]
    pub fn clear(&mut self) {
        self.hash_and_exists &= !Self::EXISTS_BIT;
    }
}

/// Open-addressing hash map with linear probing matching V8.
pub struct V8HashMap<K, V> {
    entries: Vec<HashMapEntry<K, V>>,
    capacity: usize,
    occupancy: usize,
}

impl<K: Default + Copy + PartialEq, V: Default + Copy> V8HashMap<K, V> {
    pub const DEFAULT_CAPACITY: usize = 8;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(mut cap: usize) -> Self {
        if !crate::bits::is_power_of_two_u32(cap as u32) {
            cap = crate::bits::round_up_to_power_of_two_32(cap as u32) as usize;
        }
        if cap < 2 {
            cap = 2;
        }
        let mut entries = Vec::with_capacity(cap);
        for _ in 0..cap {
            entries.push(HashMapEntry::empty());
        }
        Self {
            entries,
            capacity: cap,
            occupancy: 0,
        }
    }

    #[inline]
    pub fn occupancy(&self) -> usize {
        self.occupancy
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.occupancy == 0
    }

    pub fn clear(&mut self) {
        for e in &mut self.entries {
            *e = HashMapEntry::empty();
        }
        self.occupancy = 0;
    }

    fn probe_index(&self, key: &K, hash: u32) -> usize {
        let masked_hash = (hash & HashMapEntry::<K, V>::HASH_MASK) as usize;
        let mut i = masked_hash & (self.capacity - 1);
        while self.entries[i].exists() && !(self.entries[i].hash() == (hash & HashMapEntry::<K, V>::HASH_MASK) && self.entries[i].key == *key) {
            i = (i + 1) & (self.capacity - 1);
        }
        i
    }

    pub fn lookup(&self, key: &K, hash: u32) -> Option<&HashMapEntry<K, V>> {
        let idx = self.probe_index(key, hash);
        if self.entries[idx].exists() {
            Some(&self.entries[idx])
        } else {
            None
        }
    }

    pub fn lookup_mut(&mut self, key: &K, hash: u32) -> Option<&mut HashMapEntry<K, V>> {
        let idx = self.probe_index(key, hash);
        if self.entries[idx].exists() {
            Some(&mut self.entries[idx])
        } else {
            None
        }
    }

    pub fn lookup_or_insert(&mut self, key: K, hash: u32, default_val: V) -> &mut HashMapEntry<K, V> {
        let idx = self.probe_index(&key, hash);
        if self.entries[idx].exists() {
            return &mut self.entries[idx];
        }

        // Fill empty entry
        self.entries[idx] = HashMapEntry::new(key, default_val, hash);
        self.occupancy += 1;

        // Check load factor: grow if occupancy + occupancy / 4 >= capacity (>= 80%)
        if self.occupancy + self.occupancy / 4 >= self.capacity {
            self.resize(self.capacity * 2);
            let new_idx = self.probe_index(&key, hash);
            &mut self.entries[new_idx]
        } else {
            &mut self.entries[idx]
        }
    }

    pub fn insert_new(&mut self, key: K, hash: u32, value: V) -> &mut HashMapEntry<K, V> {
        let idx = self.probe_index(&key, hash);
        debug_assert!(!self.entries[idx].exists());
        self.entries[idx] = HashMapEntry::new(key, value, hash);
        self.occupancy += 1;

        if self.occupancy + self.occupancy / 4 >= self.capacity {
            self.resize(self.capacity * 2);
            let new_idx = self.probe_index(&key, hash);
            &mut self.entries[new_idx]
        } else {
            &mut self.entries[idx]
        }
    }

    pub fn remove(&mut self, key: &K, hash: u32) -> Option<V> {
        let idx = self.probe_index(key, hash);
        if !self.entries[idx].exists() {
            return None;
        }

        let val = self.entries[idx].value;
        let mut p = idx;
        let mut q = p;

        loop {
            q = (q + 1) & (self.capacity - 1);
            if !self.entries[q].exists() {
                break;
            }

            let r = (self.entries[q].hash() as usize) & (self.capacity - 1);
            let should_move = if q > p {
                r <= p || r > q
            } else {
                r <= p && r > q
            };

            if should_move {
                self.entries[p] = self.entries[q];
                p = q;
            }
        }

        self.entries[p].clear();
        self.occupancy -= 1;
        Some(val)
    }

    fn resize(&mut self, new_capacity: usize) {
        let old_entries = std::mem::replace(
            &mut self.entries,
            (0..new_capacity).map(|_| HashMapEntry::empty()).collect(),
        );
        let _old_cap = self.capacity;
        self.capacity = new_capacity;
        self.occupancy = 0;

        for entry in old_entries {
            if entry.exists() {
                self.insert_new(entry.key, entry.hash(), entry.value);
            }
        }
    }

    // Iteration support
    pub fn start_index(&self) -> Option<usize> {
        self.next_index(None)
    }

    pub fn next_index(&self, current: Option<usize>) -> Option<usize> {
        let start = match current {
            Some(idx) => idx + 1,
            None => 0,
        };
        for i in start..self.capacity {
            if self.entries[i].exists() {
                return Some(i);
            }
        }
        None
    }

    pub fn entry_at(&self, idx: usize) -> Option<&HashMapEntry<K, V>> {
        if idx < self.capacity && self.entries[idx].exists() {
            Some(&self.entries[idx])
        } else {
            None
        }
    }
}

impl<K: Default + Copy + PartialEq, V: Default + Copy> Default for V8HashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
