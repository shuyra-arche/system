pub(super) mod elf;

use kernel_api::model::unit::ProgramKind;

pub struct Image<'a> {
    pub bytes: &'a [u8],
    pub kind: ProgramKind,
}
