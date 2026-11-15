//! Canonical selection and retained Program demand, independent of Task state.
use crate::system::declaration::Unit;
use alloc::sync::Arc;
use system_protocol::control::instance::Sequence;
use unit_contract::document::Ending;

// A Program cannot request an execution attempt without selecting one. Stopping
// keeps the selection so observations and recovery retain its identity.
enum Selection {
    Never,
    Requested(Sequence),
    Stopped(Sequence),
}
pub(crate) struct Program {
    pub declaration: Arc<Unit>,
    selection: Selection,
}
impl Program {
    pub fn new(declaration: Arc<Unit>) -> Self {
        Self {
            declaration,
            selection: Selection::Never,
        }
    }
    pub fn current(&self) -> Option<Sequence> {
        match self.selection {
            Selection::Never => None,
            Selection::Requested(id) | Selection::Stopped(id) => Some(id),
        }
    }
    pub fn requested(&self) -> bool {
        matches!(self.selection, Selection::Requested(_))
    }
    /// Commit a successfully admitted canonical attempt. Physical startup and
    /// retirement remain Instance evidence, never inferred from this intent.
    pub fn select(&mut self, id: Sequence) {
        self.selection = Selection::Requested(id);
    }
    pub fn stop(&mut self) {
        if let Some(id) = self.current() {
            self.selection = Selection::Stopped(id);
        }
    }
    pub fn eligible(&self, retired: impl FnOnce(Sequence) -> bool) -> bool {
        self.current().is_none_or(|id| {
            retired(id)
                && (self.declaration.control().ending == Ending::Transient || !self.requested())
        })
    }
    pub fn supervised_by(&self, group: &str) -> bool {
        self.requested()
            && !self.declaration.control().guarded
            && self.declaration.control().groups.contains(&group)
    }
}
