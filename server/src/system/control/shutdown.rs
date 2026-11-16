//! Shutdown dependency selection owned by Control.
use super::program::Program;
use alloc::vec::Vec;
use system_protocol::control::instance::Sequence;

pub(super) fn stoppable(
    programs: &[Program],
    mut retired: impl FnMut(Sequence) -> bool,
) -> Vec<Sequence> {
    programs
        .iter()
        .filter_map(|provider| {
            let id = provider.current()?;
            let name = provider.declaration.name();
            let depended_on = programs.iter().any(|consumer| {
                consumer
                    .current()
                    .is_some_and(|other| other != id && !retired(other))
                    && (consumer.declaration.control().requires.contains(&name)
                        || consumer
                            .declaration
                            .control()
                            .retirement
                            .iter()
                            .any(|retirement| retirement.provider == name))
            });
            (!depended_on).then_some(id)
        })
        .collect()
}
