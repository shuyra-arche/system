use env::TaskId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureAction {
    Mint,
    Embark { parent: Option<TaskId> },
    Ruin,
    Debark,
}
