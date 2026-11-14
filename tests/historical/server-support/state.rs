#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureState {
    NeverStarted,
    Starting,
    Ready,
    Stopping,
    Debarked,
    Dead,
}
