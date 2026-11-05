use super::Accounts;
use initrd::ledger::bundle::{Archive, File, Files, Unit, Units};

/// The immutable initrd owns all borrowed image and declaration bytes.
#[derive(Clone, Copy)]
pub struct Catalog<'a> {
    archive: Archive<'a>,
}
impl<'a> Catalog<'a> {
    pub fn new(view: &'a [u8]) -> Option<Self> {
        Some(Self {
            archive: Archive::parse(view).ok()?,
        })
    }
    pub fn of_boot(accounts: &Accounts) -> Option<Catalog<'static>> {
        Catalog::new(accounts.view())
    }
    pub fn system(&self) -> Unit<'a> {
        self.archive.system()
    }
    pub fn entry(&self) -> File<'a> {
        self.archive.image()
    }
    pub fn find(&self, name: &str) -> Option<File<'a>> {
        self.archive.find(name)
    }
    pub fn entries(&self) -> Files<'a> {
        self.archive.files()
    }
    pub fn units(&self) -> Units<'a> {
        self.archive.units()
    }
}
