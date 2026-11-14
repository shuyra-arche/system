use super::image::Image;
use super::loaded_unit::LoadedUnit;
use crate::system::loader::Loader;

pub struct LoaderFixture {
    loader: Loader,
}

impl LoaderFixture {
    pub fn new() -> Self {
        Self {
            loader: Loader::new(),
        }
    }

    pub fn build(&mut self, image: Image<'_>) -> env::UnitResult<LoadedUnit> {
        self.loader
            .build(crate::system::loader::Image {
                bytes: image.bytes,
                kind: image.kind,
            })
            .map(LoadedUnit::new)
    }
}
