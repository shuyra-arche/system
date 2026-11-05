use unit::event::{Event, Exit};

/// Publication state for one constructed execution domain, independent of control state.
#[derive(Default)]
pub(crate) enum Observation {
    #[default]
    New,
    Bound,
    Ready,
    Exited,
}

impl Observation {
    pub(crate) fn bind(&mut self, unit: &str, task: u64, team: u64) -> Result<(), &'static str> {
        if !matches!(self, Self::New) {
            return Err("duplicate instance binding");
        }
        crate::system::app::events::emit(Event::Bound { unit, task, team })?;
        *self = Self::Bound;
        Ok(())
    }

    pub(crate) fn ready(&mut self, task: u64) -> Result<(), &'static str> {
        match self {
            Self::Bound => {
                crate::system::app::events::emit(Event::Ready { task })?;
                *self = Self::Ready;
                Ok(())
            }
            Self::Ready => Ok(()),
            _ => Err("instance readiness observation order"),
        }
    }

    pub(crate) fn exit(&mut self, task: u64, exit: Exit) -> Result<(), &'static str> {
        match self {
            Self::Bound | Self::Ready => {
                crate::system::app::events::emit(Event::Exited { task, exit })?;
                *self = Self::Exited;
                Ok(())
            }
            Self::Exited => Ok(()),
            Self::New => Err("unbound instance exit observation"),
        }
    }

    pub(crate) fn retired(&self, task: u64) -> Result<(), &'static str> {
        if !matches!(self, Self::Exited) {
            return Err("instance retirement without native exit observation");
        }
        crate::system::app::events::emit(Event::Retired { task })
    }
}
