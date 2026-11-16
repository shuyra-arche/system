//! Private Export admission predicates owned by Control.
use system_protocol::{identity, operator};

pub(super) fn declares_alias(aliases: &[&str], requested: &str) -> bool {
    aliases.contains(&requested)
}

pub(super) fn principal(object: &identity::IdentityObject) -> Option<identity::PrincipalId> {
    match object {
        identity::IdentityObject::Principal(principal) => Some(*principal),
        identity::IdentityObject::Coalition(_) => None,
    }
}

pub(super) fn matches_subject(
    principal: identity::PrincipalId,
    subject: &identity::Subject,
) -> bool {
    *subject.principal() == principal
}

/// Compare the resolved namespace resource with the caller's observed Operator
/// entry. A root resource is not evidence for an entry-scoped Export.
pub(super) fn matches_entry(
    binding: &operator::Binding,
    requested: &identity::OperatorEntryRef,
) -> bool {
    binding.entry.operator == requested.operator
        && binding.revision == requested.revision
        && match binding.entry.target {
            operator::EntryTarget::Node(id) => !requested.root && id == requested.id,
            operator::EntryTarget::Root => false,
        }
}
