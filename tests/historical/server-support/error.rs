#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureError {
    Room,
    Full,
    Unknown,
    NotReady,
    BadImage,
    Denied,
    Died,
    Shutdown,
}
