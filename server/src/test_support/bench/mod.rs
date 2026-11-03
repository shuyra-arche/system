use crate::system::control::unit::task::Image;
use crate::system::control::unit::verdict::Fail;
use crate::system::control::unit::table::Table;
use crate::system::loader::{Image as LoadImage, Loader, Spawn};
use env::TaskId;

mod again;
mod group;
mod load;
mod rig;

pub use again::run as run_again;
pub use group::run as run_group;
pub use load::run as run_load;
pub use rig::run as run_rig;

/// Faithful copy of the Source harness image-to-task transfer.
pub(super) fn mint(
    table: &mut Table,
    loader: &mut Loader,
    image: Image<'_>,
) -> Result<TaskId, Fail> {
    let built = loader
        .construct(
            LoadImage {
                bytes: image.bytes,
                kind: image.kind,
            },
            Spawn {
                sire: env::unit::self_id(),
                args: &[],
                stack: 0,
            },
        )
        .map_err(|_| Fail::Full)?;
    crate::system::control::unit::task::mint(table, image.name, built)
}
