//! Label abstraction and jump target backpatching for machine code assemblers.

/// A label representing a target location in an instruction stream.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Label {
    pub(crate) id: usize,
}

/// The state of a label during assembly.
#[derive(Clone, Debug)]
pub(crate) enum LabelState {
    /// Target offset has not yet been bound; holds list of jump instruction displacement offsets to patch.
    Unbound(Vec<usize>),
    /// Bound to a fixed byte offset in the emitted instruction buffer.
    Bound(usize),
}

/// Allocates and resolves labels across assembler passes.
#[derive(Clone, Debug, Default)]
pub struct LabelManager {
    labels: Vec<LabelState>,
}

impl LabelManager {
    pub fn new() -> Self {
        Self { labels: Vec::new() }
    }

    /// Creates a new unbound label.
    pub fn create_label(&mut self) -> Label {
        let id = self.labels.len();
        self.labels.push(LabelState::Unbound(Vec::new()));
        Label { id }
    }

    /// Records a reference to a label at `patch_offset` in the code buffer.
    pub fn record_use(&mut self, label: Label, patch_offset: usize) -> Option<usize> {
        match &mut self.labels[label.id] {
            LabelState::Bound(target_offset) => Some(*target_offset),
            LabelState::Unbound(uses) => {
                uses.push(patch_offset);
                None
            }
        }
    }

    /// Binds a label to the current byte offset, returning all pending jump patch offsets.
    pub fn bind(&mut self, label: Label, current_offset: usize) -> Vec<usize> {
        let prev = std::mem::replace(&mut self.labels[label.id], LabelState::Bound(current_offset));
        match prev {
            LabelState::Unbound(uses) => uses,
            LabelState::Bound(already) => {
                panic!("Label {:?} already bound to offset {}", label, already);
            }
        }
    }

    /// Checks if a label has been bound.
    pub fn is_bound(&self, label: Label) -> bool {
        matches!(self.labels.get(label.id), Some(LabelState::Bound(_)))
    }

    /// Gets the bound target offset for a label, if resolved.
    pub fn target_offset(&self, label: Label) -> Option<usize> {
        match self.labels.get(label.id) {
            Some(LabelState::Bound(target)) => Some(*target),
            _ => None,
        }
    }
}
