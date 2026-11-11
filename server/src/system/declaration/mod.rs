//! System-owned interpretation of pure schema-2 Unit records.
//!
//! These immutable values are declaration intent. Neither an Arc nor a decoded
//! Subject proves live authority: admission must still consult native services.
mod rights;
use alloc::{string::String, sync::Arc, vec::Vec};
use kernel_api::model::{identity::Mark, pend::Limits, unit::ProgramKind};
use system_protocol::{Name, Path, identity, operator, supply, value::Rights};
use unit_contract::{bundle, document as d};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Record,
    Identity,
    Image,
    Policy,
    Missing,
    Cycle,
}

#[derive(Debug)]
pub struct Loader {
    pub image: &'static str,
    pub kind: ProgramKind,
    pub args: Vec<usize>,
    pub stack: usize,
}
#[derive(Debug)]
pub struct Publication {
    pub name: Name,
    pub mark: Mark,
    pub limits: Limits,
    pub permit: operator::Permit,
}
#[derive(Debug)]
pub struct Namespace {
    pub key: &'static str,
    pub placement: d::Placement,
    pub path: Path,
    pub scope: operator::Scope,
    pub operations: operator::Ops,
    pub permit: Option<operator::Permit>,
    pub source_mark: Option<Mark>,
    pub publications: Vec<Publication>,
}
#[derive(Debug)]
pub struct Identity {
    pub aliases: Vec<&'static str>,
    pub grants: Vec<identity::Grant>,
}

/// A validated Unit owns all policy arrays; strings borrow the immutable initrd.
/// Fields cannot be mutated after validation or confused with runtime state.
#[derive(Debug)]
pub struct Unit {
    name: &'static str,
    loader: Loader,
    identity: Identity,
    namespaces: Vec<Namespace>,
    supply: Vec<supply::Binding>,
    control: d::Control<&'static str>,
}
impl Unit {
    pub fn decode(bytes: &'static [u8]) -> Result<Self, Error> {
        Self::compile(unit_contract::record::v2::decode(bytes).map_err(|_| Error::Record)?)
    }
    pub fn compile(value: d::v2::Unit<&'static str>) -> Result<Self, Error> {
        value.validate().map_err(|_| Error::Policy)?;
        // Schema validation is architecture-independent. Native identities must
        // also fit the selected Kernel's nonzero signed-register domain.
        for policy in &value.control.construction {
            if let d::Owner::Task(id) = policy.owner {
                task(id)?;
            }
            if let d::IdentitySelection::Exact(subject) = &policy.identity {
                subject_value(subject)?;
            }
        }
        let namespaces = value
            .operator
            .namespace
            .into_iter()
            .map(|ns| {
                Ok(Namespace {
                    key: ns.key,
                    placement: ns.placement,
                    path: Path::new(ns.path).ok_or(Error::Policy)?,
                    scope: match ns.scope {
                        d::Scope::Subtree => operator::Scope::Subtree,
                        d::Scope::Names(names) => operator::Scope::Names(operator::Names {
                            names: names
                                .into_iter()
                                .map(|name| Path::new(name).ok_or(Error::Policy))
                                .collect::<Result<_, _>>()?,
                        }),
                    },
                    operations: operator::Ops::new(ns.ops).ok_or(Error::Policy)?,
                    permit: ns.permit.map(permit).transpose()?,
                    source_mark: ns.source_mark.map(Mark::new),
                    publications: ns
                        .bindings
                        .into_iter()
                        .map(|binding| {
                            Ok(Publication {
                                name: Name::new(binding.name).ok_or(Error::Policy)?,
                                mark: Mark::new(binding.mark),
                                limits: rights::compile(&binding.rights).ok_or(Error::Policy)?,
                                permit: permit(binding.permit)?,
                            })
                        })
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, Error>>()?;
        let supply = value
            .supply
            .bindings
            .into_iter()
            .map(|binding| {
                Ok(supply::Binding {
                    source: supply::Key {
                        name: String::from(binding.source),
                    },
                    resource: supply::Key {
                        name: String::from(binding.resource),
                    },
                    slot: system_protocol::Mark::new(binding.slot),
                    rights: Rights(rights::compile(&binding.rights).ok_or(Error::Policy)?),
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            name: value.id,
            loader: Loader {
                image: value.loader.image,
                kind: match value.loader.space {
                    d::Space::User => ProgramKind::User,
                    d::Space::Supervisor => ProgramKind::Supervisor,
                },
                args: value
                    .loader
                    .args
                    .into_iter()
                    .map(|arg| usize::try_from(arg).map_err(|_| Error::Policy))
                    .collect::<Result<_, _>>()?,
                stack: usize::try_from(value.loader.stack).map_err(|_| Error::Policy)?,
            },
            identity: Identity {
                aliases: value.identity.aliases,
                grants: value
                    .identity
                    .grants
                    .into_iter()
                    .map(|code| {
                        <identity::Grant as weave::Code>::from_code(code).ok_or(Error::Policy)
                    })
                    .collect::<Result<_, _>>()?,
            },
            namespaces,
            supply,
            control: value.control,
        })
    }
    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn loader(&self) -> &Loader {
        &self.loader
    }
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn namespaces(&self) -> &[Namespace] {
        &self.namespaces
    }
    pub fn supply(&self) -> &[supply::Binding] {
        &self.supply
    }
    pub fn control(&self) -> &d::Control<&'static str> {
        &self.control
    }
}

/// One immutable catalog is the authority for selection and dependency names.
#[derive(Clone)]
pub struct Catalog {
    units: Arc<[Arc<Unit>]>,
    system: &'static str,
}
impl Catalog {
    pub fn decode(archive: bundle::Archive<'static>) -> Result<Self, Error> {
        kernel_api::boot::capsule::Capsule::parse(archive.capsule()).ok_or(Error::Record)?;
        let system = archive.system().id;
        let mut units = Vec::new();
        for record in archive.units() {
            let unit = Unit::decode(record.record)?;
            if unit.name() != record.id
                || units.iter().any(|old: &Arc<Unit>| old.name() == record.id)
            {
                return Err(Error::Identity);
            }
            let image = archive.find(unit.loader().image).ok_or(Error::Image)?;
            let expected = match unit.loader().kind {
                ProgramKind::User => bundle::Kind::User,
                ProgramKind::Supervisor => bundle::Kind::Supervisor,
            };
            if image.kind != expected {
                return Err(Error::Image);
            }
            units.push(Arc::new(unit));
        }
        let catalog = Self {
            units: units.into(),
            system,
        };
        let own = catalog.find(system).ok_or(Error::Missing)?;
        if own.loader().image != archive.image().name
            || own.loader().kind != ProgramKind::Supervisor
        {
            return Err(Error::Image);
        }
        catalog.validate()?;
        Ok(catalog)
    }
    fn validate(&self) -> Result<(), Error> {
        for unit in self.units.iter() {
            if unit
                .control()
                .requires
                .iter()
                .any(|name| self.find(name).is_none())
            {
                return Err(Error::Missing);
            }
            for policy in &unit.control().construction {
                if policy.unit.is_some_and(|name| self.find(name).is_none()) {
                    return Err(Error::Missing);
                }
                if let d::Owner::Unit(name) = policy.owner
                    && self.find(name).is_none()
                {
                    return Err(Error::Missing);
                }
            }
            if unit
                .control()
                .supervision
                .is_some_and(|name| self.find(name).is_none())
            {
                return Err(Error::Missing);
            }
            for retirement in &unit.control().retirement {
                let provider = self.find(retirement.provider).ok_or(Error::Missing)?;
                if usize::from(retirement.namespace) >= provider.namespaces().len() {
                    return Err(Error::Policy);
                }
            }
        }
        self.startup()?;
        Ok(())
    }
    pub fn find(&self, name: &str) -> Option<Arc<Unit>> {
        self.units.iter().find(|unit| unit.name() == name).cloned()
    }
    pub fn units(&self) -> &[Arc<Unit>] {
        &self.units
    }
    pub fn system(&self) -> &'static str {
        self.system
    }
    /// `after` constrains selected startup Units only. Requested Units remain
    /// available for dynamic construction without being accidentally started.
    pub fn startup(&self) -> Result<Vec<Arc<Unit>>, Error> {
        let mut pending: Vec<_> = self
            .units
            .iter()
            .filter(|unit| {
                unit.name() != self.system && unit.control().creation == d::Creation::Startup
            })
            .cloned()
            .collect();
        pending.sort_by_key(|unit| unit.name());
        let mut ordered = Vec::new();
        while !pending.is_empty() {
            let at = pending
                .iter()
                .position(|unit| {
                    unit.control()
                        .after
                        .iter()
                        .all(|name| !pending.iter().any(|other| other.name() == *name))
                })
                .ok_or(Error::Cycle)?;
            ordered.push(pending.remove(at));
        }
        Ok(ordered)
    }
}

fn task(raw: u64) -> Result<system_protocol::TaskId, Error> {
    if raw == 0 || raw > isize::MAX as u64 {
        return Err(Error::Identity);
    }
    Ok(system_protocol::TaskId::new(raw))
}
pub fn subject_value(value: &d::Subject) -> Result<identity::Subject, Error> {
    let authority = task(value.authority)?;
    let coalitions: Vec<_> = value
        .coalitions
        .iter()
        .map(|slot| identity::CoalitionId {
            authority,
            slot: *slot,
        })
        .collect();
    identity::Subject::new(
        identity::PrincipalId {
            authority,
            slot: value.principal,
        },
        &coalitions,
    )
    .ok_or(Error::Identity)
}
fn permit(value: d::Permit) -> Result<operator::Permit, Error> {
    use d::IdentitySelector as S;
    Ok(match value {
        d::Permit::Public => operator::Permit::Public,
        d::Permit::Bound => operator::Permit::Bound,
        d::Permit::Opener(id) => operator::Permit::Opener(system_protocol::EntryId::new(id)),
        d::Permit::Identity(selector) => operator::Permit::Identity(match selector {
            S::Exact { authority, slot } => identity::Selector::Exact(identity::PrincipalId {
                authority: task(authority)?,
                slot,
            }),
            S::DescendantOf { authority, slot } => {
                identity::Selector::DescendantOf(identity::PrincipalId {
                    authority: task(authority)?,
                    slot,
                })
            }
            S::MemberOf { authority, slot } => {
                identity::Selector::MemberOf(identity::CoalitionId {
                    authority: task(authority)?,
                    slot,
                })
            }
        }),
    })
}
