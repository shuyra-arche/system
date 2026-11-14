//! Native ELF construction. Policy and RPC admission belong to System control.
mod cache;
mod image;
mod source;
mod task;
use cache::Cache;
pub(crate) use image::{Image, ProgramKind};
use kernel_api::{
    abi::{
        call::{pend::PendFail, share_view::ShareFail, unit::UnitFail},
        memory::Failure,
    },
    model::identity::PendId,
};
pub(crate) use task::{Job, Phase, Spawn};
#[derive(Debug)]
pub(crate) enum Error {
    Invalid,
    Capacity,
    Image(image::elf::Error),
    Mapping(crate::runtime::view::Error),
    Native(Failure),
    Pend(erra::Error<PendFail>),
    Share(erra::Error<ShareFail>),
    Unit(erra::Error<UnitFail>),
}
impl Error {
    pub fn busy(&self) -> bool {
        match self {
            Self::Mapping(error) => error.busy(),
            Self::Native(Failure::Busy | Failure::Stale) => true,
            Self::Pend(error) => error.source.is_busy(),
            Self::Share(error) => error.source.is_busy(),
            Self::Unit(error) => error.source.is_busy(),
            _ => false,
        }
    }
}
pub(crate) struct Loader {
    license: PendId,
    space: PendId,
    cache: Cache,
}
impl Loader {
    /// # Safety
    /// Space is this Task's actual Internal Nest. Loader copies image bytes
    /// through private writable staging projections there.
    pub unsafe fn new(license: PendId, space: PendId) -> Result<Self, Error> {
        Ok(Self {
            license,
            space,
            cache: Cache::new(256)?,
        })
    }
    pub fn prepare(&mut self, image: Image<'_>, spawn: Spawn<'_>) -> Result<Job, Error> {
        Job::new(image, spawn)
    }
    pub fn cached_entries(&self) -> usize {
        self.cache.count()
    }
    pub fn clear_images(&mut self) -> Result<bool, Error> {
        self.cache.clear()
    }
    pub fn advance(&mut self) -> Result<bool, Error> {
        self.cache.sweep()
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid => f.write_str("invalid native image construction state"),
            Self::Capacity => f.write_str("image construction capacity exhausted"),
            Self::Image(error) => write!(f, "ELF: {error:?}"),
            Self::Mapping(error) => write!(f, "image Memo: {error:?}"),
            Self::Native(error) => write!(f, "image native memory: {error:?}"),
            Self::Pend(error) => write!(f, "image Pend: {error}"),
            Self::Share(error) => write!(f, "image Share derivation: {error}"),
            Self::Unit(error) => write!(f, "image Unit: {error}"),
        }
    }
}

fn abort(note: &'static str) -> ! {
    kernel_client::call::unit::exit(
        0,
        0x4c4f_4144,
        kernel_api::address::VirtAddr::new(note.as_ptr().addr()),
        note.len(),
        kernel_api::model::unit::ExitScope::Team,
    )
}
