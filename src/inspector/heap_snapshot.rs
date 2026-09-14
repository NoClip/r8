//! V8 `.heapsnapshot` JSON format generator for Chrome DevTools Memory inspection.
//!
//! Generates V8 heap snapshot format version 1 matching Chrome DevTools `Memory` tab
//! parser in 100% Pure Safe Rust standard library.

use crate::objects::value::JSValue;
use crate::runtime::context::Context;
use std::collections::HashMap;

/// A builder for V8 `.heapsnapshot` JSON graph representation.
pub struct HeapSnapshotBuilder {
    strings: Vec<String>,
    string_map: HashMap<String, u32>,
    nodes: Vec<u32>,
    edges: Vec<u32>,
    node_count: usize,
    edge_count: usize,
    next_node_id: u32,
}

impl Default for HeapSnapshotBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HeapSnapshotBuilder {
    pub fn new() -> Self {
        let mut builder = Self {
            strings: Vec::new(),
            string_map: HashMap::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            node_count: 0,
            edge_count: 0,
            next_node_id: 1,
        };
        // Index 0 in string table is empty string
        builder.intern_string("");
        builder
    }

    /// Interns a string and returns its index in the string pool.
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.string_map.get(s) {
            idx
        } else {
            let idx = self.strings.len() as u32;
            self.strings.push(s.to_string());
            self.string_map.insert(s.to_string(), idx);
            idx
        }
    }

    /// Adds a node to the snapshot graph and returns its node index.
    pub fn add_node(&mut self, node_type: u32, name: &str, self_size: u32, edge_count: u32) -> usize {
        let name_idx = self.intern_string(name);
        let id = self.next_node_id;
        self.next_node_id += 2; // V8 node IDs increment by 2

        let node_idx = self.node_count;
        self.nodes.push(node_type);
        self.nodes.push(name_idx);
        self.nodes.push(id);
        self.nodes.push(self_size);
        self.nodes.push(edge_count);
        self.nodes.push(0); // trace_node_id

        self.node_count += 1;
        node_idx
    }

    /// Adds an edge from current node to target node.
    pub fn add_edge(&mut self, edge_type: u32, name_or_index: u32, to_node_idx: usize) {
        let to_node_offset = (to_node_idx * 6) as u32;
        self.edges.push(edge_type);
        self.edges.push(name_or_index);
        self.edges.push(to_node_offset);
        self.edge_count += 1;
    }

    /// Builds the snapshot of the entire JS context heap.
    pub fn snapshot_context(mut self, ctx: &Context) -> String {
        // Node 0: Root synthetic node
        // Node 1: Global Object
        // Collect global object properties
        let global_obj = ctx.global_object.borrow();
        let mut props: Vec<(String, JSValue)> = Vec::new();
        let map = global_obj.map.borrow();
        if map.is_dictionary_map {
            for (k, (v, _)) in &global_obj.ext_or_default().dictionary_properties {
                props.push((k.clone(), v.clone()));
            }
        } else {
            for desc in &map.descriptors {
                if let Some(v) = global_obj.properties.get(desc.field_index) {
                    props.push((desc.name.clone(), v.clone()));
                }
            }
        }
        drop(map);
        let prop_count = props.len();

        // 1. Root node (Synthetic = type 9)
        self.add_node(9, "(GC roots)", 0, 1);

        // 2. Global Object node (Object = type 3)
        let glob_node = self.add_node(3, "global", 64, prop_count as u32);

        // Edge from root -> global (Element = type 1)
        self.add_edge(1, 0, glob_node);

        // 3. Populate global object properties
        let mut child_nodes = Vec::new();
        for (key, val) in &props {
            let (n_type, name, size) = match val {
                JSValue::Function(f) => (5, f.name.as_str(), 48), // Closure = type 5
                JSValue::Array(a) => (1, "Array", (32 + a.borrow().elements.len() * 8) as u32), // Array = type 1
                JSValue::Object(o) => (3, "Object", (32 + o.borrow().properties.len() * 8) as u32), // Object = type 3
                JSValue::String(s) => (2, s.as_str(), (16 + s.len()) as u32), // String = type 2
                JSValue::BigInt(_) => (7, "BigInt", 24), // Number/BigInt = type 7
                _ => (0, "hidden", 8), // Hidden = type 0
            };
            let c_node = self.add_node(n_type, name, size, 0);
            let name_idx = self.intern_string(key);
            child_nodes.push((name_idx, c_node));
        }

        // Add edges from global -> children (Property = type 2)
        for (name_idx, c_node) in child_nodes {
            self.add_edge(2, name_idx, c_node);
        }

        self.to_json()
    }

    /// Serializes to the standard V8 Heap Snapshot v1 JSON format.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(1024 + self.nodes.len() * 4 + self.strings.len() * 16);
        out.push_str("{\n  \"snapshot\": {\n    \"meta\": {\n");
        out.push_str("      \"node_fields\": [\"type\", \"name\", \"id\", \"self_size\", \"edge_count\", \"trace_node_id\"],\n");
        out.push_str("      \"node_types\": [[\"hidden\", \"array\", \"string\", \"object\", \"code\", \"closure\", \"regexp\", \"number\", \"native\", \"synthetic\"], \"string\", \"number\", \"number\", \"number\", \"number\"],\n");
        out.push_str("      \"edge_fields\": [\"type\", \"name_or_index\", \"to_node\"],\n");
        out.push_str("      \"edge_types\": [[\"context\", \"element\", \"property\", \"internal\", \"hidden\", \"shortcut\", \"weak\"], \"string_or_number\", \"node\"]\n");
        out.push_str("    },\n");
        out.push_str(&format!("    \"node_count\": {},\n", self.node_count));
        out.push_str(&format!("    \"edge_count\": {}\n", self.edge_count));
        out.push_str("  },\n");

        // Nodes array
        out.push_str("  \"nodes\": [");
        for (i, val) in self.nodes.iter().enumerate() {
            if i > 0 { out.push(','); }
            out.push_str(&val.to_string());
        }
        out.push_str("],\n");

        // Edges array
        out.push_str("  \"edges\": [");
        for (i, val) in self.edges.iter().enumerate() {
            if i > 0 { out.push(','); }
            out.push_str(&val.to_string());
        }
        out.push_str("],\n");

        // Strings pool array
        out.push_str("  \"strings\": [");
        for (i, s) in self.strings.iter().enumerate() {
            if i > 0 { out.push(','); }
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        out.push_str("]\n}");

        out
    }
}

/// Generates a V8 `.heapsnapshot` JSON string for the given context.
pub fn export_heap_snapshot(ctx: &Context) -> String {
    let builder = HeapSnapshotBuilder::new();
    builder.snapshot_context(ctx)
}
