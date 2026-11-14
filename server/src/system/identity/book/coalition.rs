use system_protocol::TaskId;
use system_protocol::identity::CoalitionId;
use system_protocol::identity::Fail;
use system_protocol::identity::FoundId;

use super::{IdentityBook, limits};

pub(super) struct CoalitionNode {
    pub(super) manager: system_protocol::identity::PrincipalId,
    pub(super) caller: TaskId,
    pub(super) creation: FoundId,
}

impl IdentityBook {
    pub(super) fn coalition(&self, id: CoalitionId) -> Result<usize, Fail> {
        if id.authority != self.authority {
            return Err(Fail::WrongAuthority);
        }
        let index = usize::try_from(id.slot).map_err(|_| Fail::UnknownCoalition)?;
        if index >= self.coalitions.len() {
            return Err(Fail::UnknownCoalition);
        }
        Ok(index)
    }

    pub fn found(&mut self, from: TaskId, id: FoundId) -> Result<CoalitionId, Fail> {
        let manager = *self.resolve(from).ok_or(Fail::Denied)?.current.principal();
        if let Some((slot, node)) = self
            .coalitions
            .iter()
            .enumerate()
            .find(|(_, node)| node.caller == from && node.creation == id)
        {
            if node.manager != manager {
                return Err(Fail::Changed);
            }
            return Ok(CoalitionId {
                authority: self.authority,
                slot: slot as u64,
            });
        }
        if self.coalitions.len() >= limits::MAX_COALITIONS
            || self
                .coalitions
                .iter()
                .filter(|n| n.manager == manager)
                .count()
                >= limits::MAX_CREATED_PER_PRINCIPAL
        {
            return Err(Fail::Full);
        }
        let slot = u64::try_from(self.coalitions.len()).map_err(|_| Fail::Full)?;
        slot.checked_add(1).ok_or(Fail::Full)?;
        self.coalitions.try_reserve(1).map_err(|_| Fail::Full)?;
        self.coalitions.push(CoalitionNode {
            manager,
            caller: from,
            creation: id,
        });
        Ok(CoalitionId {
            authority: self.authority,
            slot,
        })
    }
}
