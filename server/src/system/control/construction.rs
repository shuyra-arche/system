//! Bounded image import and declared construction admission; never wire authority.
use crate::runtime::view::Mapping;
use crate::system::app::services::{LIMIT_NS, Services, bound, fatal, probe, release};
use alloc::vec::Vec;
use kernel_api::{
    address::PAGE_SIZE,
    model::{
        cell::Placement,
        identity::{PendId, TaskId},
        modifiable::Modifiable,
        pend::{Operations, PendRange, PendScope, PendStatus, ShareRights, Target, ViewTarget},
    },
};
use system_protocol::loader::{self as api, Fail};

/// The offered recipient-local image grant is consumed after a private copy.
/// Sender ownership and live rights are checked before mapping; no physical
/// address or serialized foreign Pend coordinate is treated as authority.
pub(crate) fn snapshot(
    services: &Services,
    caller: TaskId,
    request: &api::ImageInput,
) -> Result<Vec<u8>, Fail> {
    if !request.valid() {
        return Err(Fail::Bad);
    }
    let source = PendId::try_from(request.image).map_err(|_| Fail::Bad)?;
    let info = probe(source).ok_or(Fail::Denied)?;
    if info.lender != caller
        || info.status != PendStatus::Active
        || info.limits.modifiable != Modifiable::No
        || !matches!(info.target, Target::View(ViewTarget::Share(_)))
        || !info
            .limits
            .operations
            .contains(Operations::Share(ShareRights::ASSOCIATE))
    {
        return Err(Fail::Denied);
    }
    let PendScope::Pages(PendRange::Pages { count, .. }) = info.limits.scope else {
        return Err(Fail::Denied);
    };
    let span = count
        .checked_mul(PAGE_SIZE)
        .filter(|bytes| *bytes <= api::MAX_IMAGE + PAGE_SIZE)
        .ok_or(Fail::Bad)?;
    let offset = usize::try_from(request.offset).map_err(|_| Fail::Bad)?;
    let len = usize::try_from(request.len).map_err(|_| Fail::Bad)?;
    if offset.checked_add(len).is_none_or(|end| end > span) {
        return Err(Fail::Bad);
    }
    use twine::view::memory::Memory;
    let mut view = Mapping::new(
        services.context.space().0,
        source,
        Placement::lowest(PAGE_SIZE).unwrap(),
        2,
        fatal,
    )
    .map_err(|_| Fail::Denied)?;
    let deadline = kernel_client::call::room::clock().saturating_add(LIMIT_NS);
    let result = (|| {
        loop {
            match view.advance() {
                Ok(true) => break,
                Ok(false) => (),
                Err(error) if error.busy() => (),
                Err(_) => return Err(Fail::Denied),
            }
            bound(deadline, "system: image import timeout");
        }
        let (address, bytes) = view.span().ok_or(Fail::Bad)?;
        if offset.checked_add(len).is_none_or(|end| end > bytes) {
            return Err(Fail::Bad);
        }
        let mut copy = Vec::new();
        copy.try_reserve_exact(len).map_err(|_| Fail::Full)?;
        for index in 0..len {
            // SAFETY: this is the caller's live, private read-only CPU
            // projection of an admitted logical Share. Its memory is outside
            // Rust allocations in this program and may change in the provider;
            // volatile loads create no shared Rust slice. Validate only the
            // resulting private snapshot, never the externally mutable source.
            copy.push(unsafe {
                core::ptr::read_volatile((address.get() + offset + index) as *const u8)
            });
        }
        Ok(copy)
    })();
    let deadline = kernel_client::call::room::clock().saturating_add(LIMIT_NS);
    loop {
        match view.finish() {
            Ok(true) => break,
            Ok(false) => (),
            Err(error) if error.busy() => (),
            Err(_) => fatal("system: image import retirement failed"),
        }
        bound(deadline, "system: image import retirement timed out");
    }
    while !release(source) {
        bound(deadline, "system: image source return timed out");
    }
    result
}
