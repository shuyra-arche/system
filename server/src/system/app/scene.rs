use super::Declarations;
use crate::system::control::unit::start::Error;
use alloc::vec::Vec;
use unit::runtime::Unit;

/// Packaged declarations remain available for runtime construction; only the
/// startup policy creates an initial instance.
pub fn programs(declarations: &Declarations) -> Result<Vec<Unit>, Error> {
    let mut pending: Vec<Unit> = declarations
        .list()
        .iter()
        .filter(|unit| {
            unit.name() != declarations.system()
                && unit.control.creation == unit::document::Creation::Startup
        })
        .cloned()
        .collect();
    pending.sort_by_key(|unit| unit.name());
    let mut ordered: Vec<Unit> = Vec::new();
    while !pending.is_empty() {
        let index = pending
            .iter()
            .position(|unit| {
                unit.control.after.as_ref().is_none_or(|after| {
                    after.iter().all(|dependency| match dependency {
                        unit::Dependency::Unit(name) => {
                            !pending.iter().any(|other| other.name() == *name)
                        }
                        _ => false,
                    })
                })
            })
            .ok_or(Error::Step("cyclic Unit readiness dependencies"))?;
        ordered.push(pending.remove(index));
    }
    Ok(ordered)
}
