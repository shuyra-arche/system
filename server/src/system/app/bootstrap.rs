use crate::boot::{Accounts, Catalog};
use crate::support::machine::Machine;
use kernel_api::{address::VirtAddr, boot::ledger::Page};
use kernel_client::call::share_view;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    BootArgs,
    Machine,
}
impl Fail {
    pub fn code(self) -> usize {
        match self { Self::BootArgs => 1, Self::Machine => 5 }
    }
}

/// Borrowed Kernel boot resources belong to the initial Task. No entry is
/// converted into a legacy capability or returned by this view of the ledger.
pub struct Boot {
    pub accounts: Accounts<'static>,
    pub machine: Machine<'static>,
    pub catalog: Catalog<'static>,
}

/// # Safety
/// `words` must be the initial Task's trusted Kernel startup arguments. Their
/// two read-only mappings, and the Kernel-minted DTB page, last for this Team.
pub unsafe fn take(words: &[usize]) -> Result<Boot, Fail> {
    // SAFETY: the entry path guarantees the Kernel startup mapping lifetime.
    let accounts = unsafe { Accounts::from_words(words) }.ok_or(Fail::BootArgs)?;
    let catalog = Catalog::of_boot(&accounts).ok_or(Fail::BootArgs)?;
    let token = accounts.page(Page::Dtb).ok_or(Fail::Machine)?;
    let (address, span) = share_view::open(token).map_err(|_| Fail::Machine)?;
    let address: VirtAddr = address;
    if address.get() == 0 || span < 40 || span > isize::MAX as usize || address.get().checked_add(span).is_none() {
        return Err(Fail::Machine);
    }
    // SAFETY: this trusted boot page is immutable Kernel DTB storage, kept
    // mapped until Team exit. The bounded slice covers only its opened extent.
    let bytes = unsafe { core::slice::from_raw_parts(address.get() as *const u8, span) };
    let machine = Machine::of(bytes).map_err(|_| Fail::Machine)?;
    Ok(Boot { accounts, machine, catalog })
}
