extern crate alloc;
#[cfg(test)]
pub(crate) use declarations::Declarations;

#[cfg(test)]
mod boot;
#[cfg(test)]
mod system;

#[cfg(test)]
#[path = "../../../server/src/system/app/declarations.rs"]
mod declarations;
#[cfg(test)]
#[path = "../../../server/src/system/app/scene.rs"]
mod scene;

#[cfg(test)]
mod tests {
    use super::{boot::Catalog, declarations::Declarations, scene};
    static VALID: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/valid.img"));
    #[test]
    fn package_membership_does_not_instantiate_requested_units() {
        let declarations = Declarations::decode(&Catalog::new(VALID).unwrap()).unwrap();
        assert_eq!(declarations.list().len(), 4);
        assert_eq!(
            declarations.find("template").unwrap().loader.image,
            "shared-bin"
        );
        let list = scene::programs(&declarations).unwrap();
        assert_eq!(
            list.iter().map(|unit| unit.name()).collect::<Vec<_>>(),
            ["provider", "caller"]
        );
        assert!(
            declarations
                .find("provider")
                .unwrap()
                .control
                .after
                .as_ref()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn malformed_runtime_selection_and_wait_cycles_are_rejected() {
        static MISSING: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/missing.img"));
        static MISMATCH: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mismatch.img"));
        static CYCLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cycle.img"));
        assert!(Declarations::decode(&Catalog::new(MISSING).unwrap()).is_err());
        assert!(Declarations::decode(&Catalog::new(MISMATCH).unwrap()).is_err());
        let declarations = Declarations::decode(&Catalog::new(CYCLE).unwrap()).unwrap();
        assert!(scene::programs(&declarations).is_err());
    }
}
