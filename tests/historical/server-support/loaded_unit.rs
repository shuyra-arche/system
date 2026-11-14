use crate::system::loader::Unit;

pub struct LoadedUnit {
    unit: Unit,
}

impl LoadedUnit {
    pub(super) fn new(unit: Unit) -> Self {
        Self { unit }
    }

    pub fn team(&self) -> env::TeamId {
        self.unit.team()
    }

    pub fn spawn(self, args: &[usize], stack: usize) -> env::UnitResult<env::TaskId> {
        self.unit.spawn(args, stack)
    }
}
