pub(super) mod elf;

/// Loader policy, not execution authority. Ordinary Kernel Team creation accepts
/// User only; privileged image kinds must be admitted by a separate provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramKind {
    User,
    Supervisor,
}

pub struct Image<'a> {
    pub bytes: &'a [u8],
    pub kind: ProgramKind,
}
