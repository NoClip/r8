//! Safe Rust reimplementation of a live WebAssembly module instance.
//!
//! Holds the runtime state of an instantiated Wasm module: linear memory,
//! tables, globals, and imported function bindings.

use super::interpreter::WasmTrap;
use super::interpreter::WasmVal;
use super::binary_parser::{WasmModule, ExportDesc};
use crate::objects::JSValue;
use crate::objects::function::JSFunction;
use std::rc::Rc;

// ─── Linear Memory ───────────────────────────────────────────────────────────

const WASM_PAGE_SIZE: usize = 65536; // 64 KiB

/// A WebAssembly linear memory.
pub struct WasmMemory {
    pub data: Vec<u8>,
    pub max_pages: Option<u32>,
    pub max_pages_64: Option<u64>,
    pub is_memory64: bool,
    pub is_shared: bool,
}

impl WasmMemory {
    pub fn new(initial_pages: u32, max_pages: Option<u32>) -> Self {
        Self::new_64(initial_pages as u64, max_pages.map(|p| p as u64), false, false)
    }

    pub fn new_64(initial_pages: u64, max_pages: Option<u64>, is_memory64: bool, is_shared: bool) -> Self {
        let initial_bytes = (initial_pages as usize).saturating_mul(WASM_PAGE_SIZE);
        Self {
            data: vec![0u8; initial_bytes],
            max_pages: max_pages.map(|p| p as u32),
            max_pages_64: max_pages,
            is_memory64,
            is_shared,
        }
    }

    pub fn size_pages(&self) -> u32 {
        (self.data.len() / WASM_PAGE_SIZE) as u32
    }

    pub fn size_pages_64(&self) -> u64 {
        (self.data.len() / WASM_PAGE_SIZE) as u64
    }

    /// Grow memory by `delta` pages. Returns old page count or -1 on failure.
    pub fn grow(&mut self, delta: u32) -> i32 {
        let old = self.size_pages();
        let new = old.checked_add(delta);
        match new {
            Some(np) => {
                if let Some(max) = self.max_pages {
                    if np > max { return -1; }
                }
                self.data.resize(np as usize * WASM_PAGE_SIZE, 0);
                old as i32
            }
            None => -1,
        }
    }

    pub fn grow_64(&mut self, delta: u64) -> i64 {
        let old = self.size_pages_64();
        let new = old.checked_add(delta);
        match new {
            Some(np) => {
                if let Some(max) = self.max_pages_64 {
                    if np > max { return -1; }
                }
                self.data.resize((np as usize).saturating_mul(WASM_PAGE_SIZE), 0);
                old as i64
            }
            None => -1,
        }
    }

    pub fn check_bounds(&self, addr: u32, size: u32) -> Result<usize, WasmTrap> {
        self.check_bounds_64(addr as u64, size)
    }

    pub fn check_bounds_64(&self, addr: u64, size: u32) -> Result<usize, WasmTrap> {
        let end = addr.checked_add(size as u64).ok_or(WasmTrap::OutOfBoundsMemoryAccess {
            addr: addr as u32, size, mem_size: self.data.len() as u32
        })?;
        if end as usize > self.data.len() {
            return Err(WasmTrap::OutOfBoundsMemoryAccess {
                addr: addr as u32, size, mem_size: self.data.len() as u32
            });
        }
        Ok(addr as usize)
    }

    pub fn load_u8(&self, addr: u32) -> Result<u8, WasmTrap> {
        self.load_u8_64(addr as u64)
    }

    pub fn load_u8_64(&self, addr: u64) -> Result<u8, WasmTrap> {
        let off = self.check_bounds_64(addr, 1)?;
        Ok(self.data[off])
    }

    pub fn load_u16(&self, addr: u32) -> Result<u16, WasmTrap> {
        self.load_u16_64(addr as u64)
    }

    pub fn load_u16_64(&self, addr: u64) -> Result<u16, WasmTrap> {
        let off = self.check_bounds_64(addr, 2)?;
        Ok(u16::from_le_bytes([self.data[off], self.data[off+1]]))
    }

    pub fn load_i32(&self, addr: u32) -> Result<i32, WasmTrap> {
        self.load_i32_64(addr as u64)
    }

    pub fn load_i32_64(&self, addr: u64) -> Result<i32, WasmTrap> {
        let off = self.check_bounds_64(addr, 4)?;
        Ok(i32::from_le_bytes([self.data[off], self.data[off+1], self.data[off+2], self.data[off+3]]))
    }

    pub fn load_i64(&self, addr: u32) -> Result<i64, WasmTrap> {
        self.load_i64_64(addr as u64)
    }

    pub fn load_i64_64(&self, addr: u64) -> Result<i64, WasmTrap> {
        let off = self.check_bounds_64(addr, 8)?;
        let bytes: [u8; 8] = self.data[off..off+8].try_into().unwrap();
        Ok(i64::from_le_bytes(bytes))
    }

    pub fn load_f32(&self, addr: u32) -> Result<f32, WasmTrap> {
        self.load_f32_64(addr as u64)
    }

    pub fn load_f32_64(&self, addr: u64) -> Result<f32, WasmTrap> {
        let off = self.check_bounds_64(addr, 4)?;
        let bytes: [u8; 4] = self.data[off..off+4].try_into().unwrap();
        Ok(f32::from_le_bytes(bytes))
    }

    pub fn load_f64(&self, addr: u32) -> Result<f64, WasmTrap> {
        self.load_f64_64(addr as u64)
    }

    pub fn load_f64_64(&self, addr: u64) -> Result<f64, WasmTrap> {
        let off = self.check_bounds_64(addr, 8)?;
        let bytes: [u8; 8] = self.data[off..off+8].try_into().unwrap();
        Ok(f64::from_le_bytes(bytes))
    }

    pub fn store_u8(&mut self, addr: u32, v: u8) -> Result<(), WasmTrap> {
        self.store_u8_64(addr as u64, v)
    }

    pub fn store_u8_64(&mut self, addr: u64, v: u8) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 1)?;
        self.data[off] = v;
        Ok(())
    }

    pub fn store_u16(&mut self, addr: u32, v: u16) -> Result<(), WasmTrap> {
        self.store_u16_64(addr as u64, v)
    }

    pub fn store_u16_64(&mut self, addr: u64, v: u16) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 2)?;
        let bytes = v.to_le_bytes();
        self.data[off] = bytes[0];
        self.data[off+1] = bytes[1];
        Ok(())
    }

    pub fn store_i32(&mut self, addr: u32, v: i32) -> Result<(), WasmTrap> {
        self.store_i32_64(addr as u64, v)
    }

    pub fn store_i32_64(&mut self, addr: u64, v: i32) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 4)?;
        let bytes = v.to_le_bytes();
        self.data[off..off+4].copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_i64(&mut self, addr: u32, v: i64) -> Result<(), WasmTrap> {
        self.store_i64_64(addr as u64, v)
    }

    pub fn store_i64_64(&mut self, addr: u64, v: i64) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 8)?;
        let bytes = v.to_le_bytes();
        self.data[off..off+8].copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_f32(&mut self, addr: u32, v: f32) -> Result<(), WasmTrap> {
        self.store_f32_64(addr as u64, v)
    }

    pub fn store_f32_64(&mut self, addr: u64, v: f32) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 4)?;
        let bytes = v.to_le_bytes();
        self.data[off..off+4].copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_f64(&mut self, addr: u32, v: f64) -> Result<(), WasmTrap> {
        self.store_f64_64(addr as u64, v)
    }

    pub fn store_f64_64(&mut self, addr: u64, v: f64) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 8)?;
        let bytes = v.to_le_bytes();
        self.data[off..off+8].copy_from_slice(&bytes);
        Ok(())
    }

    pub fn load_v128(&self, addr: u32) -> Result<[u8; 16], WasmTrap> {
        self.load_v128_64(addr as u64)
    }

    pub fn load_v128_64(&self, addr: u64) -> Result<[u8; 16], WasmTrap> {
        let off = self.check_bounds_64(addr, 16)?;
        let mut b = [0u8; 16];
        b.copy_from_slice(&self.data[off..off + 16]);
        Ok(b)
    }

    pub fn store_v128(&mut self, addr: u32, v: [u8; 16]) -> Result<(), WasmTrap> {
        self.store_v128_64(addr as u64, v)
    }

    pub fn store_v128_64(&mut self, addr: u64, v: [u8; 16]) -> Result<(), WasmTrap> {
        let off = self.check_bounds_64(addr, 16)?;
        self.data[off..off + 16].copy_from_slice(&v);
        Ok(())
    }

    // ─── Atomic Operations ───────────────────────────────────────────────────

    pub fn atomic_load_i32(&self, addr: u64) -> Result<i32, WasmTrap> {
        self.load_i32_64(addr)
    }

    pub fn atomic_load_i64(&self, addr: u64) -> Result<i64, WasmTrap> {
        self.load_i64_64(addr)
    }

    pub fn atomic_store_i32(&mut self, addr: u64, val: i32) -> Result<(), WasmTrap> {
        self.store_i32_64(addr, val)
    }

    pub fn atomic_store_i64(&mut self, addr: u64, val: i64) -> Result<(), WasmTrap> {
        self.store_i64_64(addr, val)
    }

    pub fn atomic_rmw_add_i32(&mut self, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        self.store_i32_64(addr, old.wrapping_add(val))?;
        Ok(old)
    }

    pub fn atomic_rmw_add_i64(&mut self, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        self.store_i64_64(addr, old.wrapping_add(val))?;
        Ok(old)
    }

    pub fn atomic_rmw_sub_i32(&mut self, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        self.store_i32_64(addr, old.wrapping_sub(val))?;
        Ok(old)
    }

    pub fn atomic_rmw_sub_i64(&mut self, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        self.store_i64_64(addr, old.wrapping_sub(val))?;
        Ok(old)
    }

    pub fn atomic_rmw_and_i32(&mut self, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        self.store_i32_64(addr, old & val)?;
        Ok(old)
    }

    pub fn atomic_rmw_and_i64(&mut self, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        self.store_i64_64(addr, old & val)?;
        Ok(old)
    }

    pub fn atomic_rmw_or_i32(&mut self, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        self.store_i32_64(addr, old | val)?;
        Ok(old)
    }

    pub fn atomic_rmw_or_i64(&mut self, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        self.store_i64_64(addr, old | val)?;
        Ok(old)
    }

    pub fn atomic_rmw_xor_i32(&mut self, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        self.store_i32_64(addr, old ^ val)?;
        Ok(old)
    }

    pub fn atomic_rmw_xor_i64(&mut self, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        self.store_i64_64(addr, old ^ val)?;
        Ok(old)
    }

    pub fn atomic_rmw_xchg_i32(&mut self, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        self.store_i32_64(addr, val)?;
        Ok(old)
    }

    pub fn atomic_rmw_xchg_i64(&mut self, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        self.store_i64_64(addr, val)?;
        Ok(old)
    }

    pub fn atomic_rmw_cmpxchg_i32(&mut self, addr: u64, expected: i32, replacement: i32) -> Result<i32, WasmTrap> {
        let old = self.load_i32_64(addr)?;
        if old == expected {
            self.store_i32_64(addr, replacement)?;
        }
        Ok(old)
    }

    pub fn atomic_rmw_cmpxchg_i64(&mut self, addr: u64, expected: i64, replacement: i64) -> Result<i64, WasmTrap> {
        let old = self.load_i64_64(addr)?;
        if old == expected {
            self.store_i64_64(addr, replacement)?;
        }
        Ok(old)
    }

    pub fn atomic_wait32(&self, addr: u64, expected: i32, _timeout: i64) -> Result<i32, WasmTrap> {
        let val = self.load_i32_64(addr)?;
        if val != expected {
            Ok(1) // not equal
        } else {
            Ok(0) // ok
        }
    }

    pub fn atomic_wait64(&self, addr: u64, expected: i64, _timeout: i64) -> Result<i32, WasmTrap> {
        let val = self.load_i64_64(addr)?;
        if val != expected {
            Ok(1) // not equal
        } else {
            Ok(0) // ok
        }
    }

    pub fn atomic_notify(&self, addr: u64, _count: u32) -> Result<u32, WasmTrap> {
        let _ = self.check_bounds_64(addr, 4)?;
        Ok(0)
    }
}

// ─── Table ───────────────────────────────────────────────────────────────────

/// A WebAssembly function table.
pub struct WasmTable {
    pub elements: Vec<Option<u32>>, // function indices
}

impl WasmTable {
    pub fn new(min: u32, _max: Option<u32>) -> Self {
        Self { elements: vec![None; min as usize] }
    }

    pub fn get(&self, idx: u32) -> Result<u32, WasmTrap> {
        match self.elements.get(idx as usize) {
            Some(Some(func_idx)) => Ok(*func_idx),
            Some(None) => Err(WasmTrap::UninitializedElement),
            None => Err(WasmTrap::UndefinedElement),
        }
    }

    pub fn set(&mut self, idx: u32, func_idx: Option<u32>) -> Result<(), WasmTrap> {
        match self.elements.get_mut(idx as usize) {
            Some(slot) => { *slot = func_idx; Ok(()) }
            None => Err(WasmTrap::UndefinedElement),
        }
    }
}

// ─── Global ──────────────────────────────────────────────────────────────────

/// A WebAssembly global variable slot.
pub struct WasmGlobal {
    pub value: WasmVal,
    pub mutable: bool,
}

// ─── Imported Function ───────────────────────────────────────────────────────

/// A JavaScript function imported into a Wasm module.
pub struct ImportedFunc {
    pub module: String,
    pub name: String,
    pub js_func: Rc<JSFunction>,
}

// ─── Instance ────────────────────────────────────────────────────────────────

/// A live, instantiated WebAssembly module.
pub struct WasmInstance {
    pub memories: Vec<WasmMemory>,
    pub tables: Vec<WasmTable>,
    pub globals: Vec<WasmGlobal>,
    pub imported_funcs: Vec<ImportedFunc>,
}

impl WasmInstance {
    /// Instantiate a parsed `WasmModule`, initializing memory, tables, and globals.
    pub fn instantiate(module: &WasmModule, imports: Vec<ImportedFunc>) -> Self {
        // Build memories
        let mut memories: Vec<WasmMemory> = module.memories.iter()
            .map(|m| WasmMemory::new_64(m.min, m.max, m.is_memory64, m.is_shared))
            .collect();

        // Apply data segments
        for seg in &module.data {
            let mem_idx = seg.memory_idx as usize;
            if mem_idx < memories.len() {
                let offset = if memories[mem_idx].is_memory64 {
                    eval_const_i64(&seg.offset_expr) as usize
                } else {
                    eval_const_i32(&seg.offset_expr) as usize
                };
                let mem = &mut memories[mem_idx];
                let end = offset + seg.data.len();
                if end <= mem.data.len() {
                    mem.data[offset..end].copy_from_slice(&seg.data);
                }
            }
        }

        // Build tables
        let mut tables: Vec<WasmTable> = module.tables.iter()
            .map(|t| WasmTable::new(t.min, t.max))
            .collect();

        // Apply element segments
        for seg in &module.elements {
            let tbl_idx = seg.table_idx as usize;
            if tbl_idx < tables.len() {
                let offset = eval_const_i32(&seg.offset_expr) as u32;
                let tbl = &mut tables[tbl_idx];
                for (i, &func_idx) in seg.func_indices.iter().enumerate() {
                    let _ = tbl.set(offset + i as u32, Some(func_idx));
                }
            }
        }

        // Build globals
        let globals: Vec<WasmGlobal> = module.globals.iter()
            .map(|(gt, init)| {
                let value = match gt.val_type {
                    super::binary_parser::ValType::I32 => WasmVal::I32(eval_const_i32(init)),
                    super::binary_parser::ValType::I64 => WasmVal::I64(eval_const_i64(init)),
                    super::binary_parser::ValType::F32 => WasmVal::F32(eval_const_f32(init)),
                    super::binary_parser::ValType::F64 => WasmVal::F64(eval_const_f64(init)),
                    super::binary_parser::ValType::I31Ref => WasmVal::I31(eval_const_i32(init)),
                    super::binary_parser::ValType::StructRef(_) => WasmVal::StructRef(None),
                    super::binary_parser::ValType::ArrayRef(_) => WasmVal::ArrayRef(None),
                    _ => WasmVal::FuncRef(None),
                };
                WasmGlobal { value, mutable: gt.mutable }
            })
            .collect();

        Self { memories, tables, globals, imported_funcs: imports }
    }

    // ── Memory accessors ─────────────────────────────────────────────────────

    pub fn mem_load_u8(&self, mem_idx: usize, addr: u64) -> Result<u8, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 1, mem_size: 0 })?.load_u8_64(addr)
    }
    pub fn mem_load_u16(&self, mem_idx: usize, addr: u64) -> Result<u16, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 2, mem_size: 0 })?.load_u16_64(addr)
    }
    pub fn mem_load_i32(&self, mem_idx: usize, addr: u64) -> Result<i32, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.load_i32_64(addr)
    }
    pub fn mem_load_i64(&self, mem_idx: usize, addr: u64) -> Result<i64, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.load_i64_64(addr)
    }
    pub fn mem_load_f32(&self, mem_idx: usize, addr: u64) -> Result<f32, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.load_f32_64(addr)
    }
    pub fn mem_load_f64(&self, mem_idx: usize, addr: u64) -> Result<f64, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.load_f64_64(addr)
    }
    pub fn mem_store_u8(&mut self, mem_idx: usize, addr: u64, v: u8) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 1, mem_size: 0 })?.store_u8_64(addr, v)
    }
    pub fn mem_store_u16(&mut self, mem_idx: usize, addr: u64, v: u16) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 2, mem_size: 0 })?.store_u16_64(addr, v)
    }
    pub fn mem_store_i32(&mut self, mem_idx: usize, addr: u64, v: i32) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.store_i32_64(addr, v)
    }
    pub fn mem_store_i64(&mut self, mem_idx: usize, addr: u64, v: i64) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.store_i64_64(addr, v)
    }
    pub fn mem_store_f32(&mut self, mem_idx: usize, addr: u64, v: f32) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.store_f32_64(addr, v)
    }
    pub fn mem_store_f64(&mut self, mem_idx: usize, addr: u64, v: f64) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.store_f64_64(addr, v)
    }
    pub fn mem_load_v128(&self, mem_idx: usize, addr: u64) -> Result<[u8; 16], WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 16, mem_size: 0 })?.load_v128_64(addr)
    }
    pub fn mem_store_v128(&mut self, mem_idx: usize, addr: u64, v: [u8; 16]) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 16, mem_size: 0 })?.store_v128_64(addr, v)
    }
    pub fn mem_size_pages(&self, mem_idx: usize) -> u32 {
        self.memories.get(mem_idx).map(|m| m.size_pages()).unwrap_or(0)
    }
    pub fn mem_size_pages_64(&self, mem_idx: usize) -> u64 {
        self.memories.get(mem_idx).map(|m| m.size_pages_64()).unwrap_or(0)
    }
    pub fn mem_grow(&mut self, mem_idx: usize, delta: u32) -> i32 {
        self.memories.get_mut(mem_idx).map(|m| m.grow(delta)).unwrap_or(-1)
    }
    pub fn mem_grow_64(&mut self, mem_idx: usize, delta: u64) -> i64 {
        self.memories.get_mut(mem_idx).map(|m| m.grow_64(delta)).unwrap_or(-1)
    }

    // ── Atomic accessors ─────────────────────────────────────────────────────

    pub fn mem_atomic_load_i32(&self, mem_idx: usize, addr: u64) -> Result<i32, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_load_i32(addr)
    }
    pub fn mem_atomic_load_i64(&self, mem_idx: usize, addr: u64) -> Result<i64, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_load_i64(addr)
    }
    pub fn mem_atomic_store_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_store_i32(addr, val)
    }
    pub fn mem_atomic_store_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<(), WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_store_i64(addr, val)
    }
    pub fn mem_atomic_rmw_add_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_add_i32(addr, val)
    }
    pub fn mem_atomic_rmw_add_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_add_i64(addr, val)
    }
    pub fn mem_atomic_rmw_sub_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_sub_i32(addr, val)
    }
    pub fn mem_atomic_rmw_sub_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_sub_i64(addr, val)
    }
    pub fn mem_atomic_rmw_and_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_and_i32(addr, val)
    }
    pub fn mem_atomic_rmw_and_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_and_i64(addr, val)
    }
    pub fn mem_atomic_rmw_or_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_or_i32(addr, val)
    }
    pub fn mem_atomic_rmw_or_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_or_i64(addr, val)
    }
    pub fn mem_atomic_rmw_xor_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_xor_i32(addr, val)
    }
    pub fn mem_atomic_rmw_xor_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_xor_i64(addr, val)
    }
    pub fn mem_atomic_rmw_xchg_i32(&mut self, mem_idx: usize, addr: u64, val: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_xchg_i32(addr, val)
    }
    pub fn mem_atomic_rmw_xchg_i64(&mut self, mem_idx: usize, addr: u64, val: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_xchg_i64(addr, val)
    }
    pub fn mem_atomic_rmw_cmpxchg_i32(&mut self, mem_idx: usize, addr: u64, exp: i32, repl: i32) -> Result<i32, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_rmw_cmpxchg_i32(addr, exp, repl)
    }
    pub fn mem_atomic_rmw_cmpxchg_i64(&mut self, mem_idx: usize, addr: u64, exp: i64, repl: i64) -> Result<i64, WasmTrap> {
        self.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_rmw_cmpxchg_i64(addr, exp, repl)
    }
    pub fn mem_atomic_wait32(&self, mem_idx: usize, addr: u64, expected: i32, timeout: i64) -> Result<i32, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_wait32(addr, expected, timeout)
    }
    pub fn mem_atomic_wait64(&self, mem_idx: usize, addr: u64, expected: i64, timeout: i64) -> Result<i32, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 8, mem_size: 0 })?.atomic_wait64(addr, expected, timeout)
    }
    pub fn mem_atomic_notify(&self, mem_idx: usize, addr: u64, count: u32) -> Result<u32, WasmTrap> {
        self.memories.get(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: addr as u32, size: 4, mem_size: 0 })?.atomic_notify(addr, count)
    }

    // ── Table accessors ──────────────────────────────────────────────────────

    pub fn table_get(&self, table_idx: usize, elem_idx: u32) -> Result<u32, WasmTrap> {
        self.tables.get(table_idx).ok_or(WasmTrap::UndefinedElement)?.get(elem_idx)
    }

    // ── Global accessors ─────────────────────────────────────────────────────

    pub fn global_get(&self, idx: usize) -> Result<WasmVal, WasmTrap> {
        self.globals.get(idx).map(|g| g.value.clone()).ok_or(WasmTrap::TypeMismatch)
    }

    pub fn global_set(&mut self, idx: usize, val: WasmVal) -> Result<(), WasmTrap> {
        match self.globals.get_mut(idx) {
            Some(g) if g.mutable => { g.value = val; Ok(()) }
            Some(_) => Err(WasmTrap::TypeMismatch), // immutable global
            None => Err(WasmTrap::TypeMismatch),
        }
    }

    // ── Imported function call ───────────────────────────────────────────────

    pub fn call_imported(&self, func_idx: u32, args: Vec<WasmVal>) -> Result<Vec<WasmVal>, WasmTrap> {
        let imported = self.imported_funcs.get(func_idx as usize)
            .ok_or(WasmTrap::FunctionNotFound(func_idx))?;
        // Convert WasmVal args → JSValue
        let js_args: Vec<JSValue> = args.iter().map(wasm_val_to_js).collect();
        match imported.js_func.call(&JSValue::Undefined, &js_args) {
            Ok(result) => Ok(vec![js_val_to_wasm(result)]),
            Err(_msg) => Err(WasmTrap::Unreachable), // trap on JS error
        }
    }
}

// ─── Const-expr evaluator ────────────────────────────────────────────────────

/// Evaluate a constant expression (global/data segment offset) → i32.
pub fn eval_const_i32(expr: &[u8]) -> i32 {
    if expr.len() >= 2 && expr[0] == 0x41 {
        // i32.const leb128...
        let mut result = 0i32;
        let mut shift = 0u32;
        for &b in &expr[1..] {
            if b == 0x0B { break; }
            result |= ((b & 0x7F) as i32) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                if shift < 32 && (b & 0x40) != 0 { result |= !0i32 << shift; }
                break;
            }
        }
        result
    } else {
        0
    }
}

/// Evaluate a constant expression → i64.
pub fn eval_const_i64(expr: &[u8]) -> i64 {
    if expr.len() >= 2 && expr[0] == 0x42 {
        let mut result = 0i64;
        let mut shift = 0u32;
        for &b in &expr[1..] {
            if b == 0x0B { break; }
            result |= ((b & 0x7F) as i64) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                if shift < 64 && (b & 0x40) != 0 { result |= !0i64 << shift; }
                break;
            }
        }
        result
    } else {
        0
    }
}

/// Evaluate a constant expression → f32.
pub fn eval_const_f32(expr: &[u8]) -> f32 {
    if expr.len() >= 5 && expr[0] == 0x43 {
        let bytes: [u8; 4] = [expr[1], expr[2], expr[3], expr[4]];
        f32::from_le_bytes(bytes)
    } else {
        0.0
    }
}

/// Evaluate a constant expression → f64.
pub fn eval_const_f64(expr: &[u8]) -> f64 {
    if expr.len() >= 9 && expr[0] == 0x44 {
        let bytes: [u8; 8] = expr[1..9].try_into().unwrap_or([0u8; 8]);
        f64::from_le_bytes(bytes)
    } else {
        0.0
    }
}

// ─── Value conversion helpers ─────────────────────────────────────────────────

pub fn wasm_val_to_js(v: &WasmVal) -> JSValue {
    match v {
        WasmVal::I32(n) => JSValue::Smi(*n),
        WasmVal::I64(n) => {
            if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                JSValue::Smi(*n as i32)
            } else {
                JSValue::Number(*n as f64)
            }
        }
        WasmVal::F32(f) => JSValue::Number(*f as f64),
        WasmVal::F64(f) => JSValue::Number(*f),
        WasmVal::FuncRef(_) => JSValue::Null,
        WasmVal::ExternRef(None) => JSValue::Null,
        WasmVal::ExternRef(Some(idx)) => JSValue::Smi(*idx as i32),
        WasmVal::V128(bytes) => {
            let list: Vec<JSValue> = bytes.iter().map(|&b| JSValue::Smi(b as i32)).collect();
            JSValue::Object(crate::objects::JSArray::new_array(list))
        }
        WasmVal::StructRef(None) => JSValue::Null,
        WasmVal::StructRef(Some(s)) => {
            let s_borrow = s.borrow();
            let obj = crate::objects::js_object::JSObject::new_empty(None);
            for (i, val) in s_borrow.fields.iter().enumerate() {
                crate::objects::js_object::JSObject::set_property(
                    &obj,
                    &format!("${}", i),
                    wasm_val_to_js(val),
                );
            }
            JSValue::Object(obj)
        }
        WasmVal::ArrayRef(None) => JSValue::Null,
        WasmVal::ArrayRef(Some(a)) => {
            let a_borrow = a.borrow();
            let list: Vec<JSValue> = a_borrow.elements.iter().map(wasm_val_to_js).collect();
            JSValue::Object(crate::objects::JSArray::new_array(list))
        }
        WasmVal::I31(n) => JSValue::Smi(*n),
    }
}

pub fn js_val_to_wasm(v: JSValue) -> WasmVal {
    match v {
        JSValue::Smi(n) => WasmVal::I32(n),
        JSValue::Number(f) => WasmVal::F64(f),
        JSValue::Boolean(b) => WasmVal::I32(if b { 1 } else { 0 }),
        JSValue::Null | JSValue::Undefined => WasmVal::ExternRef(None),
        _ => WasmVal::I32(0),
    }
}

/// Build the exports object from an instantiated module.
pub fn build_exports_object(
    module: &WasmModule,
    instance: std::rc::Rc<std::cell::RefCell<WasmInstance>>,
) -> std::rc::Rc<std::cell::RefCell<crate::objects::js_object::JSObject>> {
    use crate::objects::js_object::JSObject;
    let exports = JSObject::new_empty(None);

    for exp in &module.exports {
        match exp.desc {
            ExportDesc::Func(func_idx) => {
                let inst_clone = instance.clone();
                let module_clone = module.clone();
                let fi = func_idx;
                let func = JSFunction::new_closure(&exp.name, move |_this, args| {
                    let expected_params = module_clone.func_type(fi).map(|ft| ft.params.clone());
                    // Convert JS args → WasmVal with type guidance
                    let wasm_args: Vec<WasmVal> = args.iter().enumerate().map(|(i, a)| {
                        let expected = expected_params.as_ref().and_then(|p| p.get(i));
                        match expected {
                            Some(super::binary_parser::ValType::I32) => {
                                match a {
                                    JSValue::Smi(n) => WasmVal::I32(*n),
                                    JSValue::Number(f) => WasmVal::I32(*f as i32),
                                    JSValue::Boolean(b) => WasmVal::I32(if *b { 1 } else { 0 }),
                                    _ => WasmVal::I32(0),
                                }
                            }
                            Some(super::binary_parser::ValType::I64) => {
                                match a {
                                    JSValue::Smi(n) => WasmVal::I64(*n as i64),
                                    JSValue::Number(f) => WasmVal::I64(*f as i64),
                                    JSValue::Boolean(b) => WasmVal::I64(if *b { 1 } else { 0 }),
                                    _ => WasmVal::I64(0),
                                }
                            }
                            Some(super::binary_parser::ValType::F32) => {
                                match a {
                                    JSValue::Smi(n) => WasmVal::F32(*n as f32),
                                    JSValue::Number(f) => WasmVal::F32(*f as f32),
                                    _ => WasmVal::F32(0.0),
                                }
                            }
                            Some(super::binary_parser::ValType::F64) => {
                                match a {
                                    JSValue::Smi(n) => WasmVal::F64(*n as f64),
                                    JSValue::Number(f) => WasmVal::F64(*f),
                                    _ => WasmVal::F64(0.0),
                                }
                            }
                            Some(super::binary_parser::ValType::ExternRef) => {
                                match a {
                                    JSValue::Null | JSValue::Undefined => WasmVal::ExternRef(None),
                                    JSValue::Smi(n) => WasmVal::ExternRef(Some(*n as u32)),
                                    _ => WasmVal::ExternRef(Some(1)),
                                }
                            }
                            Some(super::binary_parser::ValType::FuncRef) => {
                                match a {
                                    JSValue::Null | JSValue::Undefined => WasmVal::FuncRef(None),
                                    JSValue::Smi(n) => WasmVal::FuncRef(Some(*n as u32)),
                                    _ => WasmVal::FuncRef(None),
                                }
                            }
                            _ => {
                                match a {
                                    JSValue::Smi(n) => WasmVal::I32(*n),
                                    JSValue::Number(f) => {
                                        if f.fract() == 0.0 && *f >= i32::MIN as f64 && *f <= i32::MAX as f64 {
                                            WasmVal::I32(*f as i32)
                                        } else {
                                            WasmVal::F64(*f)
                                        }
                                    }
                                    JSValue::Boolean(b) => WasmVal::I32(if *b { 1 } else { 0 }),
                                    _ => WasmVal::I32(0),
                                }
                            }
                        }
                    }).collect();

                    let mut inst = inst_clone.borrow_mut();
                    match super::interpreter::WasmInterpreter::call_func(&module_clone, &mut inst, fi, wasm_args, 0) {
                        Ok(results) => {
                            if results.is_empty() {
                                Ok(JSValue::Undefined)
                            } else {
                                Ok(wasm_val_to_js(&results[0]))
                            }
                        }
                        Err(trap) => Err(format!("WebAssembly trap: {}", trap)),
                    }
                });
                JSObject::set_property(&exports, &exp.name, JSValue::Function(func));
            }
            ExportDesc::Memory(mem_idx) => {
                let mem_data = {
                    let inst = instance.borrow();
                    inst.memories.get(mem_idx as usize).map(|m| m.data.clone()).unwrap_or_default()
                };
                let mem_obj = JSObject::new_empty(None);
                let ab = JSObject::new_array_buffer_from_bytes(mem_data, None);
                JSObject::set_property(&mem_obj, "buffer", JSValue::Object(ab));
                JSObject::set_property(&mem_obj, "__type__", JSValue::String("WebAssembly.Memory".to_string()));
                JSObject::set_property(&exports, &exp.name, JSValue::Object(mem_obj));
            }
            ExportDesc::Table(_) | ExportDesc::Global(_) | ExportDesc::Tag(_) => {
                // Placeholder — full TypedArray/Global/Tag support in Phase 20/40
            }
        }
    }

    exports
}
