use env::ProgramKind;

pub struct Image<'a> {
    pub bytes: &'a [u8],
    pub kind: ProgramKind,
}
