use crate::boot::{Accounts, Catalog};
use crate::runtime::view::Read;
use crate::support::machine::Machine;
use alloc::{boxed::Box, vec::Vec};
use kernel_api::{
    address::VirtAddr,
    boot::ledger::{Name, Page},
    model::unit::ExitScope,
};
use kernel_client::call::{room, unit};

// One immutable initial-Task snapshot, retained for the System Team lifetime.
// Machine's existing static borrows cannot outlive this storage. The temporary
// CPU projection is explicitly retired before this snapshot is published.
static TREE: spin::Once<Box<[u8]>> = spin::Once::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    BootArgs,
    Machine,
}
impl Fail {
    pub fn code(self) -> usize {
        match self {
            Self::BootArgs => 1,
            Self::Machine => 5,
        }
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
    let bytes = TREE.try_call_once(|| snapshot(&accounts))?;
    let machine = Machine::of(bytes).map_err(|_| Fail::Machine)?;
    Ok(Boot {
        accounts,
        machine,
        catalog,
    })
}

fn snapshot(accounts: &Accounts<'_>) -> Result<Box<[u8]>, Fail> {
    let token = accounts.page(Page::Dtb).ok_or(Fail::Machine)?;
    let Some(crate::boot::Resource::Nest(space)) = accounts.resource(Name::Space) else {
        return Err(Fail::BootArgs);
    };
    let deadline = room::clock().saturating_add(5_000_000_000);
    let mut view = loop {
        // SAFETY: both grants come from this initial Task's trusted ledger;
        // Space is its current Internal Nest and Dtb is immutable Kernel data.
        match unsafe { Read::new(space, token, abort) } {
            Ok(view) => break view,
            Err(error) if error.busy() && room::clock() < deadline => room::yield_(),
            Err(_) => return Err(Fail::Machine),
        }
    };
    let result = (|| {
        loop {
            match view.advance() {
                Ok(true) => break,
                Ok(false) => (),
                Err(error) if error.busy() => (),
                Err(_) => return Err(Fail::Machine),
            }
            if room::clock() >= deadline {
                return Err(Fail::Machine);
            }
            room::yield_();
        }
        // SAFETY: the borrowed Kernel boot source is immutable, held by this
        // Task and cannot be withdrawn by another Unit during bootstrap. This
        // private read-only projection remains published through the copy.
        let bytes = unsafe { view.bytes() }.map_err(|_| Fail::Machine)?;
        if bytes.len() < 40 {
            return Err(Fail::Machine);
        }
        let len = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        if !(40..=bytes.len()).contains(&len) {
            return Err(Fail::Machine);
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(len).map_err(|_| Fail::Machine)?;
        owned.extend_from_slice(&bytes[..len]);
        Machine::of(&owned).map_err(|_| Fail::Machine)?;
        Ok(owned)
    })();
    // Even parse/allocation/open failure retains its exact acquired reservation
    // until Release and Return settle. Never drop the native cleanup owner.
    let deadline = room::clock().saturating_add(5_000_000_000);
    loop {
        match view.finish() {
            Ok(true) => break,
            Ok(false) => (),
            Err(error) if error.busy() => (),
            Err(_) => abort("system: boot mapping retirement failed"),
        }
        if room::clock() >= deadline {
            abort("system: boot mapping retirement timeout");
        }
        room::yield_();
    }
    result.map(Vec::into_boxed_slice)
}
fn abort(note: &'static str) -> ! {
    unit::exit(
        0,
        5,
        VirtAddr::new(note.as_ptr().addr()),
        note.len(),
        ExitScope::Team,
    )
}
