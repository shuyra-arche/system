use env::TaskId;
use unit::UnitFile;

pub type SupplyHook = for<'a> fn(Option<TaskId>, &'a UnitFile, TaskId) -> Result<(), &'static str>;
