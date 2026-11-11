use alloc::{sync::Arc, vec::Vec};
use unit::runtime::{Unit, UnitFile};

/// Clones share native owned declarations; strings refer to genuine boot bytes.
#[derive(Clone)]
pub(crate) struct Declarations {
    units: Arc<[Unit]>,
    system: &'static str,
}

impl Declarations {
    pub(crate) fn decode(catalog: &crate::boot::Catalog<'static>) -> Result<Self, &'static str> {
        let mut units: Vec<Unit> = Vec::new();
        for entry in catalog.units() {
            let mut document =
                unit::record::decode(entry.record).map_err(|_| "invalid Unit record")?;
            if document.id != entry.id {
                return Err("Unit record identity mismatch");
            }
            if document
                .control
                .requires
                .iter()
                .any(|id| !catalog.units().any(|entry| entry.id == *id))
            {
                return Err("required Unit missing from initrd");
            }
            document.control.after.retain(|id| {
                *id != catalog.system().id && catalog.units().any(|entry| entry.id == *id)
            });
            let image = catalog
                .find(document.loader.image)
                .ok_or("Unit image missing")?;
            let expected = match document.loader.space {
                unit::document::Space::User => unit_contract::bundle::Kind::User,
                unit::document::Space::Supervisor => unit_contract::bundle::Kind::Supervisor,
            };
            if image.kind != expected {
                return Err("Unit image execution space mismatch");
            }
            units.push(Arc::new(
                UnitFile::try_from(document).map_err(|_| "invalid native Unit")?,
            ));
        }
        let system = catalog.system().id;
        let result = Self {
            units: units.into(),
            system,
        };
        let boot = result.find(system).ok_or("System declaration missing")?;
        if boot.loader.image != catalog.entry().name {
            return Err("System boot image mismatch");
        }
        Ok(result)
    }
    pub(crate) fn find(&self, name: &str) -> Option<Unit> {
        self.units
            .iter()
            .find(|program| program.name() == name)
            .cloned()
    }
    pub(crate) fn list(&self) -> &[Unit] {
        &self.units
    }
    pub(crate) fn system(&self) -> &'static str {
        self.system
    }
}
