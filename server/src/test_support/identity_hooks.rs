use super::{Fixture, SupplyHook};
use env::TaskId;
use system_api::identity::CoalitionId;
use system_api::operator::{Path, PathBuf};
use system_client::identity::Face;
use system_client::operator::Face as Operator;

pub struct IdentityHooks {
    pub supply: SupplyHook,
    pub account: fn(&mut Fixture, &Operator),
    pub hub_activate:
        fn(&Face, TaskId, &[CoalitionId]) -> Result<(), ()>,
    pub router_entry: &'static Path,
    pub rtc_entry: &'static Path,
    pub uart_rx: fn() -> PathBuf,
}
