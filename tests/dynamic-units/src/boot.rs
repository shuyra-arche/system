#[path = "../../../support/src/boot/catalog.rs"]
mod catalog;
pub use catalog::Catalog;
pub struct Accounts;
impl Accounts {
    pub fn view(&self) -> &'static [u8] {
        &[]
    }
}
