#[derive(Default, Clone, Copy)]
pub(super) struct Tally {
    pub(super) n: usize,
    pub(super) now: usize,
    pub(super) waited: usize,
    pub(super) late: usize,
    pub(super) lost: usize,
}
