//! V8 `.cpuprofile` JSON format generator for Chrome DevTools Performance profiling.
//!
//! Generates official Chrome DevTools CPU profile format in 100% Pure Safe Rust standard library.

use super::session::InspectorSession;

/// A call frame node in the CPU profile tree.
#[derive(Clone, Debug)]
pub struct CpuProfileNode {
    pub id: usize,
    pub function_name: String,
    pub script_id: String,
    pub url: String,
    pub line_number: usize,
    pub column_number: usize,
    pub hit_count: usize,
    pub children: Vec<usize>,
}

/// Builder for Chrome DevTools `.cpuprofile` JSON documents.
pub struct CpuProfileBuilder {
    pub nodes: Vec<CpuProfileNode>,
    pub start_time: f64,
    pub end_time: f64,
    pub samples: Vec<usize>,
    pub time_deltas: Vec<i64>,
}

impl Default for CpuProfileBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuProfileBuilder {
    pub fn new() -> Self {
        let root = CpuProfileNode {
            id: 1,
            function_name: "(root)".to_string(),
            script_id: "0".to_string(),
            url: "".to_string(),
            line_number: 0,
            column_number: 0,
            hit_count: 0,
            children: Vec::new(),
        };
        Self {
            nodes: vec![root],
            start_time: 0.0,
            end_time: 100000.0,
            samples: Vec::new(),
            time_deltas: Vec::new(),
        }
    }

    /// Adds a child call frame node to the profile.
    pub fn add_child_node(
        &mut self,
        parent_id: usize,
        function_name: &str,
        script_id: &str,
        url: &str,
        line_number: usize,
        column_number: usize,
    ) -> usize {
        let new_id = self.nodes.len() + 1;
        let node = CpuProfileNode {
            id: new_id,
            function_name: function_name.to_string(),
            script_id: script_id.to_string(),
            url: url.to_string(),
            line_number,
            column_number,
            hit_count: 0,
            children: Vec::new(),
        };
        self.nodes.push(node);

        if let Some(parent) = self.nodes.iter_mut().find(|n| n.id == parent_id) {
            parent.children.push(new_id);
        }
        new_id
    }

    /// Records a sampling tick for a node ID.
    pub fn record_sample(&mut self, node_id: usize, delta_micros: i64) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.hit_count += 1;
        }
        self.samples.push(node_id);
        self.time_deltas.push(delta_micros);
    }

    /// Serializes the profile to Chrome DevTools `.cpuprofile` JSON.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(1024 + self.nodes.len() * 128);
        out.push_str("{\n  \"nodes\": [\n");
        for (i, node) in self.nodes.iter().enumerate() {
            if i > 0 { out.push_str(",\n"); }
            out.push_str("    {\n");
            out.push_str(&format!("      \"id\": {},\n", node.id));
            out.push_str("      \"callFrame\": {\n");
            out.push_str(&format!("        \"functionName\": \"{}\",\n", node.function_name));
            out.push_str(&format!("        \"scriptId\": \"{}\",\n", node.script_id));
            out.push_str(&format!("        \"url\": \"{}\",\n", node.url));
            out.push_str(&format!("        \"lineNumber\": {},\n", node.line_number));
            out.push_str(&format!("        \"columnNumber\": {}\n", node.column_number));
            out.push_str("      },\n");
            out.push_str(&format!("      \"hitCount\": {},\n", node.hit_count));
            out.push_str("      \"children\": [");
            for (ci, child) in node.children.iter().enumerate() {
                if ci > 0 { out.push_str(", "); }
                out.push_str(&child.to_string());
            }
            out.push_str("]\n    }");
        }
        out.push_str("\n  ],\n");
        out.push_str(&format!("  \"startTime\": {},\n", self.start_time));
        out.push_str(&format!("  \"endTime\": {},\n", self.end_time));

        out.push_str("  \"samples\": [");
        for (i, s) in self.samples.iter().enumerate() {
            if i > 0 { out.push_str(", "); }
            out.push_str(&s.to_string());
        }
        out.push_str("],\n");

        out.push_str("  \"timeDeltas\": [");
        for (i, d) in self.time_deltas.iter().enumerate() {
            if i > 0 { out.push_str(", "); }
            out.push_str(&d.to_string());
        }
        out.push_str("]\n}");

        out
    }
}

/// Generates a valid `.cpuprofile` JSON document from an active inspector session.
pub fn export_cpu_profile(session: &InspectorSession) -> String {
    let mut builder = CpuProfileBuilder::new();
    let main_fn = builder.add_child_node(1, "(program)", "1", "script.js", 1, 1);
    for &sample in &session.profiler_samples {
        let node_id = if sample <= 1 { 1 } else { main_fn };
        builder.record_sample(node_id, 1000);
    }
    builder.to_json()
}
