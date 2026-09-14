//! Safe Rust reimplementation of Google V8's `src/base/small-vector.h`.
//!
//! Provides `SmallVector<T, const N: usize>`, a small-buffer optimized vector
//! that maintains elements inline on the stack up to capacity `N`, seamlessly
//! spilling to heap storage upon exceeding inline capacity.

use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut, Index, IndexMut};

/// Calculates the new capacity according to V8's growth formula:
/// `RoundUpToPowerOfTwo(std::max(min_capacity, 2 * capacity()))`
#[inline]
pub fn calc_growth(capacity: usize, min_capacity: usize) -> usize {
    let candidate = std::cmp::max(min_capacity, capacity.saturating_mul(2));
    crate::bits::round_up_to_power_of_two_32(candidate as u32) as usize
}

/// SmallVector with small-buffer optimization.
pub struct SmallVector<T, const N: usize> {
    inline: [MaybeUninit<T>; N],
    heap: Option<Vec<T>>,
    len: usize,
}

impl<T, const N: usize> SmallVector<T, N> {
    /// Creates a new empty SmallVector.
    #[inline]
    pub const fn new() -> Self {
        // SAFETY: An uninitialized array of MaybeUninit is valid.
        let inline = unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() };
        Self {
            inline,
            heap: None,
            len: 0,
        }
    }

    /// Creates a SmallVector initialized with `size` copies of `value`.
    pub fn with_size(size: usize, value: T) -> Self
    where
        T: Clone,
    {
        let mut vec = Self::new();
        vec.resize(size, value);
        vec
    }

    /// Returns the number of elements in the vector.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the vector contains no elements.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total capacity before further reallocation is needed.
    #[inline]
    pub fn capacity(&self) -> usize {
        match &self.heap {
            Some(h) => h.capacity(),
            None => N,
        }
    }

    /// Returns true if the vector has spilled to heap storage.
    #[inline]
    pub const fn is_big(&self) -> bool {
        self.heap.is_some()
    }

    /// Reserves capacity for at least `min_capacity` elements.
    pub fn reserve(&mut self, min_capacity: usize) {
        if min_capacity <= self.capacity() {
            return;
        }
        let new_cap = calc_growth(self.capacity(), min_capacity);
        self.spill_to_heap(new_cap);
    }

    fn spill_to_heap(&mut self, target_capacity: usize) {
        if let Some(h) = &mut self.heap {
            if target_capacity > h.capacity() {
                let additional = target_capacity - h.len();
                h.reserve(additional);
            }
        } else {
            let mut h = Vec::with_capacity(target_capacity);
            for i in 0..self.len {
                // SAFETY: Elements 0..self.len have been initialized and are moved to heap.
                let elem = unsafe { self.inline[i].assume_init_read() };
                h.push(elem);
            }
            self.heap = Some(h);
        }
    }

    /// Appends an element to the end of the vector.
    pub fn push(&mut self, value: T) {
        if self.len < N && self.heap.is_none() {
            self.inline[self.len] = MaybeUninit::new(value);
            self.len += 1;
        } else {
            if self.heap.is_none() {
                let new_cap = calc_growth(N, N + 1);
                self.spill_to_heap(new_cap);
            }
            let h = self.heap.as_mut().unwrap();
            h.push(value);
            self.len = h.len();
        }
    }

    /// Removes and returns the last element, or None if empty.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        if let Some(h) = &mut self.heap {
            let val = h.pop();
            self.len = h.len();
            val
        } else {
            self.len -= 1;
            // SAFETY: Element at index self.len was previously initialized and is now removed.
            let val = unsafe { self.inline[self.len].assume_init_read() };
            Some(val)
        }
    }

    /// Removes `count` elements from the back.
    pub fn pop_back_n(&mut self, count: usize) {
        assert!(count <= self.len, "SmallVector::pop_back_n count exceeds len");
        if let Some(h) = &mut self.heap {
            h.truncate(self.len - count);
            self.len = h.len();
        } else {
            for i in (self.len - count)..self.len {
                // SAFETY: Elements in range [self.len - count .. self.len] were initialized and are dropped.
                unsafe {
                    std::ptr::drop_in_place(self.inline[i].as_mut_ptr());
                }
            }
            self.len -= count;
        }
    }

    /// Inserts an element at position `index`.
    pub fn insert(&mut self, index: usize, value: T) {
        assert!(index <= self.len, "SmallVector::insert index out of bounds");
        if self.len == self.capacity() {
            self.reserve(self.len + 1);
        }
        if let Some(h) = &mut self.heap {
            h.insert(index, value);
            self.len = h.len();
        } else {
            // Shift elements [index..len] to the right
            // SAFETY: Bounds check asserted `index <= self.len`. `self.len < self.capacity()`.
            // Memory range [index..self.len] is initialized and non-overlapping with destination.
            unsafe {
                let p = self.inline.as_mut_ptr() as *mut T;
                std::ptr::copy(p.add(index), p.add(index + 1), self.len - index);
                std::ptr::write(p.add(index), value);
            }
            self.len += 1;
        }
    }

    /// Removes and returns the element at position `index`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "SmallVector::remove index out of bounds");
        if let Some(h) = &mut self.heap {
            let val = h.remove(index);
            self.len = h.len();
            val
        } else {
            // SAFETY: Bounds asserted `index < self.len`. Element at index is read,
            // and subsequent elements are shifted left by 1 position.
            unsafe {
                let p = self.inline.as_mut_ptr() as *mut T;
                let val = std::ptr::read(p.add(index));
                std::ptr::copy(p.add(index + 1), p.add(index), self.len - index - 1);
                self.len -= 1;
                val
            }
        }
    }

    /// Resizes the vector to `new_size`. If increasing, fills with `value`.
    pub fn resize(&mut self, new_size: usize, value: T)
    where
        T: Clone,
    {
        if new_size > self.len {
            if new_size > self.capacity() {
                self.reserve(new_size);
            }
            while self.len < new_size {
                self.push(value.clone());
            }
        } else {
            self.pop_back_n(self.len - new_size);
        }
    }

    /// Clears the vector, removing all elements without reverting back to inline storage.
    pub fn clear(&mut self) {
        if let Some(h) = &mut self.heap {
            h.clear();
        } else {
            for i in 0..self.len {
                // SAFETY: Elements 0..self.len are initialized and dropped in place.
                unsafe {
                    std::ptr::drop_in_place(self.inline[i].as_mut_ptr());
                }
            }
        }
        self.len = 0;
    }

    /// Returns a slice over the elements.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        if let Some(h) = &self.heap {
            h.as_slice()
        } else {
            // SAFETY: Elements 0..self.len are initialized, contiguous, and aligned.
            unsafe {
                let ptr = self.inline.as_ptr() as *const T;
                std::slice::from_raw_parts(ptr, self.len)
            }
        }
    }

    /// Returns a mutable slice over the elements.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        if let Some(h) = &mut self.heap {
            h.as_mut_slice()
        } else {
            // SAFETY: Elements 0..self.len are initialized, contiguous, aligned, and uniquely borrowed.
            unsafe {
                let ptr = self.inline.as_mut_ptr() as *mut T;
                std::slice::from_raw_parts_mut(ptr, self.len)
            }
        }
    }
}

impl<T, const N: usize> Default for SmallVector<T, N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone, const N: usize> Clone for SmallVector<T, N> {
    fn clone(&self) -> Self {
        let mut copy = Self::new();
        if self.is_big() {
            copy.reserve(self.capacity());
        }
        for item in self.as_slice() {
            copy.push(item.clone());
        }
        copy
    }
}

impl<T, const N: usize> Deref for SmallVector<T, N> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for SmallVector<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T, const N: usize> Index<usize> for SmallVector<T, N> {
    type Output = T;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.as_slice()[index]
    }
}

impl<T, const N: usize> IndexMut<usize> for SmallVector<T, N> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.as_mut_slice()[index]
    }
}

impl<T: std::fmt::Debug, const N: usize> std::fmt::Debug for SmallVector<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice().iter()).finish()
    }
}

impl<T, const N: usize> Drop for SmallVector<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}
