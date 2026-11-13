//! Native ELF construction. Policy and RPC admission belong to System control.
mod bootstrap;
mod cache;
mod image;
pub(crate) mod memory;
mod source;
mod task;
use cache::Cache;
pub(crate) use image::{Image, ProgramKind};
use kernel_api::{
    abi::call::{
        cell::CellFail, pend::PendFail, share_view::ShareFail, sign::SignFail, unit::UnitFail,
    },
    model::identity::PendId,
};
pub(crate) use task::{Job, Phase, Spawn};
#[derive(Debug)]
pub(crate) enum Error {
    Invalid,
    Sign(erra::Error<SignFail>),
    Capacity,
    Image(image::elf::Error),
    Mapping(memory::Error),
    Cell(erra::Error<CellFail>),
    Share(erra::Error<ShareFail>),
    Pend(erra::Error<PendFail>),
    Unit(erra::Error<UnitFail>),
}
impl Error {
    pub fn busy(&self) -> bool {
        match self {
            Self::Sign(error) => error.source.is_busy(),
            Self::Mapping(error) => error.busy(),
            Self::Cell(error) => error.source.is_busy(),
            Self::Share(error) => error.source.is_busy(),
            Self::Pend(error) => error.source.is_busy(),
            Self::Unit(error) => error.source.is_busy(),
            _ => false,
        }
    }
}
pub(crate) struct Loader {
    license: PendId,
    cache: Cache,
}
impl Loader {
    pub fn new(license: PendId) -> Result<Self, Error> {
        Ok(Self {
            license,
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
            Self::Sign(error) => write!(f, "image Sign: {error}"),
            Self::Invalid => f.write_str("invalid native image construction state"),
            Self::Capacity => f.write_str("image construction capacity exhausted"),
            Self::Image(error) => write!(f, "ELF: {error:?}"),
            Self::Mapping(error) => write!(f, "image Memo: {error:?}"),
            Self::Cell(error) => write!(f, "image Cell: {error}"),
            Self::Share(error) => write!(f, "image Share: {error}"),
            Self::Pend(error) => write!(f, "image Pend: {error}"),
            Self::Unit(error) => write!(f, "image Unit: {error}"),
        }
    }
}
