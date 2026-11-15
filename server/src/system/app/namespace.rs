//! System's namespace plans. Operator remains the authority for every mutation.
use super::services::Services;
use alloc::{string::String, vec::Vec};
use kernel_api::model::identity::Mark;
use system_protocol::{Name, Path, operator as op, value::Outcome};

pub(crate) struct Layout {
    next: u64,
    keys: Vec<(String, op::Entry)>,
}
impl Layout {
    pub fn new() -> Self {
        Self {
            next: 1,
            keys: Vec::new(),
        }
    }
    pub(crate) fn number(&mut self) -> u64 {
        let id = self.next;
        self.next = id
            .checked_add(1)
            .unwrap_or_else(|| super::services::fatal("system: namespace sequence exhausted"));
        id
    }
    pub fn root(&self, services: &Services) -> op::Entry {
        op::Entry {
            operator: services.operator().into(),
            target: op::EntryTarget::Root,
        }
    }
    pub fn part(
        &mut self,
        services: &Services,
        parent: op::Entry,
        name: &str,
    ) -> Result<op::Binding, op::Fail> {
        let request = op::Part {
            change: op::Mutation {
                id: op::OperationId::new(self.number()).unwrap(),
                entry: parent,
                expected: 0,
            },
            name: Name::new(name).ok_or(op::Fail::Bad)?,
            permit: op::Permit::Public,
        };
        result(services.mutate::<op::authority::operations::Part>(request))
    }
    pub fn key(&mut self, services: &Services, name: &str) -> Result<op::Entry, op::Fail> {
        if let Some((_, entry)) = self.keys.iter().find(|(key, _)| key == name) {
            return Ok(entry.clone());
        }
        self.keys.try_reserve(1).map_err(|_| op::Fail::Full)?;
        let entry = self.part(services, self.root(services), name)?.entry;
        result(
            services.mutate::<op::authority::operations::Associate>(op::Association {
                mark: Mark::of(name).into(),
                name: Name::new(name).ok_or(op::Fail::Bad)?,
                entry: entry.clone(),
            }),
        )?;
        let resolved = result(services.query::<op::operations::Resolve>(op::Resolve {
            mark: Mark::of(name).into(),
            name: Name::new(name).ok_or(op::Fail::Bad)?,
        }))?;
        if resolved != entry {
            return Err(op::Fail::Conflict);
        }
        self.keys.push((String::from(name), entry.clone()));
        Ok(entry)
    }
    pub fn path(
        &mut self,
        services: &Services,
        mut parent: op::Entry,
        path: &Path,
    ) -> Result<op::Entry, op::Fail> {
        if !path.as_str().is_empty() {
            for name in path.as_str().split('/') {
                parent = self.part(services, parent, name)?.entry;
            }
        }
        Ok(parent)
    }
    pub fn resource(
        &self,
        services: &Services,
        base: op::Entry,
        name: &str,
    ) -> Result<op::Resource, op::Fail> {
        let binding = result(
            services.mutate::<op::authority::operations::Find>(op::Query {
                anchor: base,
                name: Name::new(name).ok_or(op::Fail::Bad)?,
            }),
        )?;
        result(services.mutate::<op::authority::operations::Read>(op::At {
            entry: binding.entry,
        }))
    }
}

pub(crate) fn result<T, E>(value: Outcome<T, E>) -> Result<T, E> {
    match value {
        Outcome::Success(value) => Ok(value),
        Outcome::Failure(error) => Err(error),
    }
}
