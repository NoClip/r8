//! Safe Rust reimplementation of Google V8's Pointer Compression (`v8_enable_pointer_compression`).
//!
//! On 64-bit platforms, standard 8-byte pointer representation wastes data cache and memory bandwidth.
//! V8 pointer compression anchors all heap objects within a 4GB-aligned isolate reservation, storing
//! references as 32-bit compressed offsets with Smi and weak reference tagging, halving pointer overhead.

use crate::objects::value::JSValue;

/// 32-bit compressed pointer storing an offset relative to an `IsolateRoot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct CompressedPointer(pub u32);

impl CompressedPointer {
    /// Tag mask for distinguishing Smi vs HeapObject.
    pub const TAG_MASK: u32 = 0b01;
    pub const SMI_TAG: u32 = 0b00;
    pub const HEAP_OBJECT_TAG: u32 = 0b01;

    /// Weak reference modifier mask on heap object pointers.
    pub const WEAK_MASK: u32 = 0b10;
    pub const STRONG_TAG: u32 = 0b01;
    pub const WEAK_TAG: u32 = 0b11;

    /// Checks whether this compressed pointer encodes an inline Smi integer.
    #[inline(always)]
    pub fn is_smi(&self) -> bool {
        (self.0 & Self::TAG_MASK) == Self::SMI_TAG
    }

    /// Checks whether this compressed pointer encodes a heap object reference.
    #[inline(always)]
    pub fn is_heap_object(&self) -> bool {
        (self.0 & Self::TAG_MASK) == Self::HEAP_OBJECT_TAG
    }

    /// Checks whether this heap object pointer is a weak reference.
    #[inline(always)]
    pub fn is_weak(&self) -> bool {
        (self.0 & 0b11) == Self::WEAK_TAG
    }

    /// Encodes a 31-bit signed integer into a Smi compressed representation.
    #[inline(always)]
    pub fn from_smi(val: i32) -> Self {
        Self(((val as u32) << 1) & !Self::TAG_MASK)
    }

    /// Decodes an inline Smi integer from its compressed representation.
    #[inline(always)]
    pub fn to_smi(&self) -> i32 {
        (self.0 as i32) >> 1
    }

    /// Marks this heap object pointer as a weak reference.
    #[inline(always)]
    pub fn make_weak(&self) -> Self {
        Self(self.0 | Self::WEAK_MASK)
    }

    /// Clears the weak reference bit, returning a strong pointer.
    #[inline(always)]
    pub fn make_strong(&self) -> Self {
        Self(self.0 & !Self::WEAK_MASK)
    }

    /// Returns the raw 32-bit payload.
    #[inline(always)]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

/// 64-bit base anchor address representing the 4GB Isolate heap region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsolateRoot {
    pub base_address: u64,
}

impl Default for IsolateRoot {
    fn default() -> Self {
        Self::new(0x0000_7FFF_0000_0000)
    }
}

impl IsolateRoot {
    /// 4GB isolate region mask.
    pub const REGION_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4 GB
    pub const ALIGN_MASK: u64 = !(Self::REGION_SIZE - 1);

    /// Creates a new `IsolateRoot` aligned to a 4GB boundary.
    pub fn new(base_address: u64) -> Self {
        Self {
            base_address: base_address & Self::ALIGN_MASK,
        }
    }

    /// Compresses a 64-bit host memory pointer into a 32-bit `CompressedPointer`.
    #[inline(always)]
    pub fn compress(&self, full_ptr: u64) -> CompressedPointer {
        let offset = full_ptr.saturating_sub(self.base_address);
        CompressedPointer((offset as u32 & !0b11) | CompressedPointer::HEAP_OBJECT_TAG)
    }

    /// Decompresses a 32-bit `CompressedPointer` back to its 64-bit host memory address.
    #[inline(always)]
    pub fn decompress(&self, comp: CompressedPointer) -> u64 {
        let offset = (comp.0 & !0b11) as u64;
        self.base_address + offset
    }

    /// Compresses a 64-bit pointer with optional weak reference tagging.
    #[inline(always)]
    pub fn compress_weak(&self, full_ptr: u64, is_weak: bool) -> CompressedPointer {
        let tag = if is_weak { CompressedPointer::WEAK_TAG } else { CompressedPointer::STRONG_TAG };
        let offset = full_ptr.saturating_sub(self.base_address);
        CompressedPointer((offset as u32 & !0b11) | tag)
    }
}

/// Compact 32-bit word encoding standard JavaScript runtime values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompressedValue {
    Undefined,
    Null,
    Boolean(bool),
    Smi(i32),
    HeapObject(CompressedPointer),
}

impl CompressedValue {
    // Special sentinel bit patterns (upper 4 bits tag)
    pub const TAG_SPECIAL: u32 = 0xF000_0000;
    pub const VAL_UNDEFINED: u32 = Self::TAG_SPECIAL | 0x01;
    pub const VAL_NULL: u32 = Self::TAG_SPECIAL | 0x02;
    pub const VAL_FALSE: u32 = Self::TAG_SPECIAL | 0x03;
    pub const VAL_TRUE: u32 = Self::TAG_SPECIAL | 0x04;

    /// Compresses a `JSValue` into a compact `CompressedValue`.
    pub fn compress(val: &JSValue, root: &IsolateRoot) -> Self {
        match val {
            JSValue::Undefined => CompressedValue::Undefined,
            JSValue::Null => CompressedValue::Null,
            JSValue::Boolean(b) => CompressedValue::Boolean(*b),
            JSValue::Smi(i) => CompressedValue::Smi(*i),
            JSValue::Object(rc) => {
                let ptr = std::rc::Rc::as_ptr(rc) as usize as u64;
                CompressedValue::HeapObject(root.compress(ptr))
            }
            JSValue::Array(rc) => {
                let ptr = std::rc::Rc::as_ptr(rc) as usize as u64;
                CompressedValue::HeapObject(root.compress(ptr))
            }
            JSValue::Number(f) => {
                // If float represents an exact integer fitting in Smi, compress as Smi
                if f.fract() == 0.0 && *f >= (i32::MIN as f64) && *f <= (i32::MAX as f64) {
                    CompressedValue::Smi(*f as i32)
                } else {
                    CompressedValue::Undefined
                }
            }
            _ => CompressedValue::Undefined,
        }
    }

    /// Decompresses this `CompressedValue` back to a `JSValue`.
    pub fn decompress(&self) -> JSValue {
        match self {
            CompressedValue::Undefined => JSValue::Undefined,
            CompressedValue::Null => JSValue::Null,
            CompressedValue::Boolean(b) => JSValue::Boolean(*b),
            CompressedValue::Smi(i) => JSValue::Smi(*i),
            CompressedValue::HeapObject(p) => {
                if p.is_smi() {
                    JSValue::Smi(p.to_smi())
                } else {
                    JSValue::Undefined
                }
            }
        }
    }

    /// Packs this compressed value into a single 32-bit integer word.
    pub fn to_raw_u32(&self) -> u32 {
        match self {
            CompressedValue::Undefined => Self::VAL_UNDEFINED,
            CompressedValue::Null => Self::VAL_NULL,
            CompressedValue::Boolean(false) => Self::VAL_FALSE,
            CompressedValue::Boolean(true) => Self::VAL_TRUE,
            CompressedValue::Smi(i) => CompressedPointer::from_smi(*i).raw(),
            CompressedValue::HeapObject(p) => p.raw(),
        }
    }

    /// Unpacks a 32-bit raw integer word into a `CompressedValue`.
    pub fn from_raw_u32(raw: u32) -> Self {
        match raw {
            Self::VAL_UNDEFINED => CompressedValue::Undefined,
            Self::VAL_NULL => CompressedValue::Null,
            Self::VAL_FALSE => CompressedValue::Boolean(false),
            Self::VAL_TRUE => CompressedValue::Boolean(true),
            _ => {
                let p = CompressedPointer(raw);
                if p.is_smi() {
                    CompressedValue::Smi(p.to_smi())
                } else {
                    CompressedValue::HeapObject(p)
                }
            }
        }
    }
}

/// A compact memory page storing compressed 32-bit pointer slots with 50% memory savings.
#[derive(Clone, Debug)]
pub struct CompressedHeapPage {
    pub root: IsolateRoot,
    pub slots: Vec<u32>,
}

impl CompressedHeapPage {
    /// Allocates a new compressed heap page with the given number of 32-bit slots.
    pub fn new(root: IsolateRoot, slot_count: usize) -> Self {
        Self {
            root,
            slots: vec![CompressedValue::VAL_UNDEFINED; slot_count],
        }
    }

    /// Stores a JavaScript value in the compressed slot.
    #[inline]
    pub fn store(&mut self, slot: usize, val: &JSValue) {
        if slot < self.slots.len() {
            let comp = CompressedValue::compress(val, &self.root);
            self.slots[slot] = comp.to_raw_u32();
        }
    }

    /// Loads a JavaScript value from the compressed slot.
    #[inline]
    pub fn load(&self, slot: usize) -> JSValue {
        if slot < self.slots.len() {
            CompressedValue::from_raw_u32(self.slots[slot]).decompress()
        } else {
            JSValue::Undefined
        }
    }

    /// Total memory size in bytes consumed by this compressed page (32 bits per slot).
    #[inline]
    pub fn compressed_bytes(&self) -> usize {
        self.slots.len() * std::mem::size_of::<u32>()
    }

    /// Total memory size in bytes that would be consumed without pointer compression (64 bits per slot).
    #[inline]
    pub fn uncompressed_bytes(&self) -> usize {
        self.slots.len() * std::mem::size_of::<u64>()
    }

    /// Memory savings percentage achieved (always ~50%).
    #[inline]
    pub fn memory_savings_ratio(&self) -> f64 {
        1.0 - (self.compressed_bytes() as f64 / self.uncompressed_bytes() as f64)
    }
}
