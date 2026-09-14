//! Safe Rust reimplementation of Google V8`s `src/base/vector.h`.
//!
//! Provides non-owning slices (`VectorView`), owning boxed slices (`OwnedVector`),
//! and utility functions with 1:1 observable behavior matching V8.

use std::ops::{Deref, DerefMut, Index};

/// Non-owning view over contiguous memory, corresponding to V8`s `v8::base::Vector<T>`.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct VectorView<'a, T> {
    data: *const T,
    length: usize,
    _marker: std::marker::PhantomData<&'a T>,
}

unsafe impl<'a, T: Sync> Sync for VectorView<'a, T> {}
unsafe impl<'a, T: Send> Send for VectorView<'a, T> {}

impl<'a, T> VectorView<'a, T> {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            data: std::ptr::null(),
            length: 0,
            _marker: std::marker::PhantomData,
        }
    }

    #[inline]
    pub const fn new(data: *const T, length: usize) -> Self {
        debug_assert!(length == 0 || !data.is_null());
        Self {
            data,
            length,
            _marker: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn from_slice(slice: &'a [T]) -> Self {
        Self {
            data: slice.as_ptr(),
            length: slice.len(),
            _marker: std::marker::PhantomData,
        }
    }

    #[inline]
    pub const fn size(&self) -> usize {
        self.length
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    #[inline]
    pub const fn as_ptr(&self) -> *const T {
        self.data
    }

    #[inline]
    pub fn as_slice(&self) -> &'a [T] {
        if self.length == 0 || self.data.is_null() {
            &[]
        } else {
            // SAFETY: The pointer `self.data` is guaranteed valid for reads of `self.length` elements
            // for lifetime `'a` as established at construction time, properly aligned and non-aliasing.
            unsafe { std::slice::from_raw_parts(self.data, self.length) }
        }
    }

    #[inline]
    pub fn sub_vector(&self, from: usize, to: usize) -> Self {
        assert!(from <= to);
        assert!(to <= self.length);
        if from >= to {
            return Self::empty();
        }
        // SAFETY: Bounds are asserted: `from < to <= self.length`.
        // Therefore `from` is within the contiguous memory allocation.
        let ptr = unsafe { self.data.add(from) };
        Self::new(ptr, to - from)
    }

    #[inline]
    pub fn sub_vector_from(&self, from: usize) -> Self {
        self.sub_vector(from, self.length)
    }

    #[inline]
    pub fn first(&self) -> &T {
        debug_assert!(self.length > 0);
        &self.as_slice()[0]
    }

    #[inline]
    pub fn last(&self) -> &T {
        debug_assert!(self.length > 0);
        &self.as_slice()[self.length - 1]
    }

    #[inline]
    pub fn at(&self, index: usize) -> &T {
        debug_assert!(index < self.length);
        &self.as_slice()[index]
    }

    #[inline]
    pub fn truncate(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.length);
        self.length = new_len;
    }
}

impl<'a, T: PartialEq> PartialEq for VectorView<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a, T: Eq> Eq for VectorView<'a, T> {}

impl<'a, T> Index<usize> for VectorView<'a, T> {
    type Output = T;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.at(index)
    }
}

/// Owning vector, corresponding to V8`s `v8::base::OwnedVector<T>`.
#[derive(Debug)]
pub struct OwnedVector<T> {
    data: Box<[T]>,
}

impl<T> OwnedVector<T> {
    #[inline]
    pub fn empty() -> Self {
        Self {
            data: Box::new([]),
        }
    }

    #[inline]
    pub fn from_boxed_slice(boxed: Box<[T]>) -> Self {
        Self { data: boxed }
    }

    #[inline]
    pub fn from_vec(vec: Vec<T>) -> Self {
        Self {
            data: vec.into_boxed_slice(),
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline]
    pub fn as_vector(&self) -> VectorView<'_, T> {
        VectorView::from_slice(&self.data)
    }

    #[inline]
    pub fn release(self) -> Box<[T]> {
        self.data
    }
}

impl<T: Default + Clone> OwnedVector<T> {
    #[inline]
    pub fn new(size: usize) -> Self {
        if size == 0 {
            return Self::empty();
        }
        let vec = vec![T::default(); size];
        Self::from_vec(vec)
    }

    #[inline]
    pub fn new_with_value(size: usize, init: T) -> Self {
        if size == 0 {
            return Self::empty();
        }
        let vec = vec![init; size];
        Self::from_vec(vec)
    }
}

impl<T: Clone> OwnedVector<T> {
    #[inline]
    pub fn new_by_copying(data: &[T]) -> Self {
        Self::from_vec(data.to_vec())
    }
}

impl<T> Deref for OwnedVector<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for OwnedVector<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: PartialEq> PartialEq for OwnedVector<T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<T: Eq> Eq for OwnedVector<T> {}

