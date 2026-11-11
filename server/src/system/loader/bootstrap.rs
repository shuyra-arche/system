//! A one-shot target-side initializer for writable mappings under Kernel ABI 9.
//! Only immutable loader code is projected by the parent; writable Share grants
//! are associated and published by the receiving Task in its own CPU space.
use super::Error;
use alloc::vec::Vec;
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{
            CellCreate, CellInfo, CellObject, CellPosition, CellSource, CellState, CreatedCell,
            MemoDestination, Placement,
        },
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{Operations, PendInfo, PendStatus, ShareRights, Target, ViewTarget},
        unit::{ExitScope, UnitInfo, UnitQuery},
    },
};
use kernel_client::call::{cell, pend, room, share_view, sign, unit};
pub(super) const MARK: Mark = Mark::of("shuyra/system/loader/startup/v9");
const MAGIC: usize = 0x3954_5241_5453_5953;
const HEADER: usize = 5;
const RECORD: usize = 4;

pub(super) fn metadata(
    owner: TaskId,
    entry: usize,
    ready: PendId,
    regions: &[[usize; RECORD]],
) -> Result<Vec<u8>, Error> {
    let words = HEADER
        .checked_add(regions.len().checked_mul(RECORD).ok_or(Error::Invalid)?)
        .ok_or(Error::Invalid)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(words.checked_mul(8).ok_or(Error::Invalid)?)
        .map_err(|_| Error::Capacity)?;
    for word in [MAGIC, owner.get(), entry, ready.get(), regions.len()] {
        bytes.extend_from_slice(&(word as u64).to_le_bytes());
    }
    for region in regions {
        for word in region {
            bytes.extend_from_slice(&(*word as u64).to_le_bytes());
        }
    }
    Ok(bytes)
}

#[cfg(target_arch = "riscv64")]
pub(super) fn code() -> Result<(&'static [u8], usize, usize), Error> {
    unsafe extern "C" {
        static __code_start: u8;
        static __code_end: u8;
    }
    let begin = (&raw const __code_start).addr();
    let end = (&raw const __code_end).addr();
    if begin == 0 || !begin.is_multiple_of(PAGE_SIZE) || end <= begin {
        return Err(Error::Invalid);
    }
    let size = (end - begin)
        .checked_next_multiple_of(PAGE_SIZE)
        .ok_or(Error::Invalid)?;
    // SAFETY: the native entry linker defines the immutable complete RX span.
    let bytes = unsafe { core::slice::from_raw_parts(begin as *const u8, end - begin) };
    Ok((bytes, begin, size))
}
#[cfg(not(target_arch = "riscv64"))]
pub(super) fn code() -> Result<(&'static [u8], usize, usize), Error> {
    Err(Error::Invalid)
}

fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}
fn id(raw: usize) -> PendId {
    PendId::from_bytes(&(raw as u64).to_le_bytes()).unwrap()
}
macro_rules! stop {
    () => {
        fail(line!() as u64)
    };
}
fn fail(at: u64) -> ! {
    unit::exit(at, 0x4c4f_4144, VirtAddr::new(0), 0, ExitScope::Task)
}
fn probe() -> UnitInfo {
    let mut bytes = [0; UnitInfo::WIDTH];
    match unit::inspect(UnitQuery::Caller, VirtAddr::new(bytes.as_mut_ptr().addr())) {
        Ok(true) => UnitInfo::from_bytes(&bytes).unwrap_or_else(|| stop!()),
        _ => stop!(),
    }
}
fn meta(owner: TaskId) -> PendId {
    let mut after = PendId::NONE;
    let mut found = None;
    loop {
        let mut bytes = [0; PendInfo::SIZE];
        let count = match pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1) {
            Ok(value) => value,
            Err(error) if error.source.is_busy() => {
                room::yield_();
                continue;
            }
            Err(_) => stop!(),
        };
        if count == 0 {
            return found.unwrap_or_else(|| stop!());
        }
        let info = PendInfo::from_bytes(&bytes).unwrap_or_else(|| stop!());
        after = info.token;
        if info.mark != MARK {
            continue;
        }
        if found.is_some()
            || info.owner != owner
            || info.lender != owner
            || info.status != PendStatus::Active
            || !matches!(info.target, Target::View(ViewTarget::Share(_)))
            || info.limits.modifiable != Modifiable::No
            || !matches!(info.limits.operations, Operations::Share(rights) if rights.contains(ShareRights::OPEN | ShareRights::SHUT))
        {
            stop!();
        }
        found = Some(info.token);
    }
}
fn publish(token: PendId, pages: usize) {
    for page in 0..pages {
        loop {
            let mut position = CellPosition {
                object: CellObject::Memo(token),
                page,
                generation: 0,
            };
            let mut bytes = [0; CellInfo::WIDTH];
            match cell::inspect(
                addr(&position.to_bytes()),
                VirtAddr::new(bytes.as_mut_ptr().addr()),
            ) {
                Ok(()) => (),
                Err(error) if error.source.is_busy() => {
                    room::yield_();
                    continue;
                }
                Err(_) => stop!(),
            }
            let info = CellInfo::from_bytes(&bytes).unwrap_or_else(|| stop!());
            position.generation = info.generation;
            match info.state {
                CellState::Published => break,
                CellState::Ready => match cell::publish(addr(&position.to_bytes())) {
                    Ok(()) => (),
                    Err(error) if error.source.is_busy() => (),
                    Err(_) => stop!(),
                },
                CellState::Publishing => (),
                _ => stop!(),
            }
            room::yield_();
        }
    }
}

fn read_metadata<const N: usize>(token: PendId, offset: usize) -> [usize; N] {
    let (base, size) = loop {
        match share_view::open(token) {
            Ok(value) => break value,
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(_) => stop!(),
        }
    };
    if base.get() == 0
        || size > isize::MAX as usize
        || offset
            .checked_add(N)
            .and_then(|n| n.checked_mul(8))
            .is_none_or(|end| end > size)
    {
        stop!();
    }
    let mut out = [0; N];
    {
        // SAFETY: the authenticated creator retains this immutable Share until
        // the explicit quiescence signal. The borrow ends before Shut below.
        let bytes = unsafe { core::slice::from_raw_parts(base.get() as *const u8, size) };
        for (index, value) in out.iter_mut().enumerate() {
            *value = u64::from_le_bytes(
                bytes[(offset + index) * 8..(offset + index + 1) * 8]
                    .try_into()
                    .unwrap(),
            ) as usize;
        }
    }
    loop {
        match share_view::shut(token) {
            Ok(()) => break,
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(_) => stop!(),
        }
    }
    out
}

/// Kernel starts this immutable initializer in the target's privilege/space.
/// The creator-owned read-only metadata is the only source of mapping requests.
pub(super) unsafe extern "C" fn enter(words: *const usize, count: usize) -> ! {
    if !(3..=system_protocol::loader::MAX_ARGS + 3).contains(&count) {
        stop!();
    }
    // SAFETY: Kernel copied count startup words to this Task's own guarded stack.
    let words = unsafe { core::slice::from_raw_parts(words, count) };
    let owner = TaskId::new(words[0]);
    let entry = words[1];
    let argc = words[2];
    if owner.get() == 0 || argc != count - 3 || probe().creator != owner {
        stop!();
    }
    let metadata = meta(owner);
    let header = read_metadata::<HEADER>(metadata, 0);
    if header[0] != MAGIC || header[1] != owner.get() || header[2] != entry {
        stop!();
    }
    let ready = id(header[3]);
    let count = header[4];
    if count > 2 * u16::MAX as usize {
        stop!();
    }
    for index in 0..count {
        // Open chooses a temporary free VA. Close this borrow before installing
        // a target range, since that VA may be the program's next DATA address.
        let record = read_metadata::<RECORD>(metadata, HEADER + index * RECORD);
        let [source, address, pages, access] = record;
        if source == 0 || pages == 0 || access != 6 || !address.is_multiple_of(PAGE_SIZE) {
            stop!();
        }
        let request = CellCreate::Memo {
            source: CellSource::Pend {
                token: id(source),
                offset: 0,
                count: pages,
            },
            destination: MemoDestination::Internal {
                team: kernel_api::model::identity::TeamId::new(0),
                placement: Placement::exact(VirtAddr::new(address), PAGE_SIZE)
                    .unwrap_or_else(|| stop!()),
                access: access as u64,
            },
        }
        .to_bytes();
        let token = loop {
            match cell::create(PendId::NONE, addr(&request)) {
                Ok(CreatedCell::Memo { token, .. }) => break token,
                Err(error) if error.source.is_busy() => room::yield_(),
                Err(error) => fail(1000 + (-(error.source as isize)) as u64),
                _ => stop!(),
            }
        };
        publish(token, pages);
    }
    loop {
        match pend::return_(metadata) {
            Ok(()) => break,
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(_) => stop!(),
        }
    }
    // All metadata reads and borrows are complete before this signal. The
    // parent may reclaim its metadata Share immediately after observing it.
    loop {
        match sign::trigger(ready) {
            Ok(()) => break,
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(_) => stop!(),
        }
    }
    #[cfg(target_arch = "riscv64")]
    unsafe {
        core::arch::asm!("andi sp, a0, -16", "jr a2", in("a0") words.as_ptr().add(3), in("a1") argc, in("a2") entry, options(noreturn));
    }
    #[cfg(not(target_arch = "riscv64"))]
    stop!()
}
