use alloc::string::String;
use kernel_api::{
    abi::call::clone_view::CloneFail,
    model::{
        clone_view::CloneLimits,
        identity::{Mark, PendId},
    },
};

#[derive(Debug)]
pub enum Error {
    Name,
    Memory,
    Create(erra::Error<CloneFail>),
}

/// Create one bounded native face. The caller owns the returned queue and must
/// finish all receivers/watches before withdrawing it with Pend Return.
pub fn entry(
    license: PendId,
    mark: Mark,
    name: &str,
    limits: CloneLimits,
) -> Result<(PendId, String), Error> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(Error::Name);
    }
    let mut owned = String::new();
    owned
        .try_reserve_exact(name.len())
        .map_err(|_| Error::Memory)?;
    owned.push_str(name);
    let queue =
        kernel_client::call::clone_view::create(license, mark, limits).map_err(Error::Create)?;
    Ok((queue, owned))
}
