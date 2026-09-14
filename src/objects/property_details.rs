//! Safe Rust reimplementation of Google V8's `src/objects/property-details.h`.
//!
//! Encapsulates property attributes, representation, kind, and constness.

pub mod attributes {
    pub const NONE: u8 = 0;
    pub const READ_ONLY: u8 = 1 << 0;
    pub const DONT_ENUM: u8 = 1 << 1;
    pub const DONT_DELETE: u8 = 1 << 2;
}

pub type PropertyAttributes = u8;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PropertyKind {
    Data,
    Accessor,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PropertyLocation {
    InObject,
    OutOfObject,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PropertyConstness {
    Mutable,
    Const,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Representation {
    None,
    Smi,
    Double,
    HeapObject,
    Tagged,
}

/// Details about an object property: attributes, kind, location, and representation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PropertyDetails {
    pub kind: PropertyKind,
    pub location: PropertyLocation,
    pub constness: PropertyConstness,
    pub attributes: u8,
    pub representation: Representation,
}

impl Default for PropertyDetails {
    fn default() -> Self {
        Self {
            kind: PropertyKind::Data,
            location: PropertyLocation::InObject,
            constness: PropertyConstness::Mutable,
            attributes: attributes::NONE,
            representation: Representation::Tagged,
        }
    }
}

impl PropertyDetails {
    pub fn new_data(attributes: u8, location: PropertyLocation) -> Self {
        Self {
            kind: PropertyKind::Data,
            location,
            constness: PropertyConstness::Mutable,
            attributes,
            representation: Representation::Tagged,
        }
    }

    pub fn is_read_only(&self) -> bool {
        (self.attributes & attributes::READ_ONLY) != 0
    }

    pub fn is_dont_enum(&self) -> bool {
        (self.attributes & attributes::DONT_ENUM) != 0
    }

    pub fn is_dont_delete(&self) -> bool {
        (self.attributes & attributes::DONT_DELETE) != 0
    }
}
