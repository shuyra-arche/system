use kernel_api::{
    boot::capsule,
    model::{
        modifiable::Modifiable,
        pend::{
            Distributable, NestRights, NestScope, Operations, PendRange, PendScope, ShareRights,
        },
    },
};
use system_declaration_tests::declaration::{Catalog, Error, Unit};
use unit_contract::{bundle, document as d};

fn document(id: &'static str) -> d::v2::Unit<&'static str> {
    d::v2::Unit {
        schema: 2,
        id,
        loader: d::Loader {
            image: id,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn rights(operations: d::rights::Operations) -> d::Rights {
    d::Rights {
        operations,
        modifiable: false,
        distributable: false,
        window: d::rights::Window::Inherit,
    }
}
fn binding(rights: d::Rights) -> d::v2::SupplyBinding<&'static str> {
    d::v2::SupplyBinding {
        source: "system/initrd",
        resource: "payload",
        slot: 7,
        rights,
    }
}
fn capsule() -> Vec<u8> {
    let mut bytes = vec![0; 2 * capsule::PAGE];
    bytes[..8].copy_from_slice(&capsule::MAGIC);
    bytes[8..10].copy_from_slice(&capsule::VERSION.to_le_bytes());
    bytes[10] = 12;
    bytes[11] = kernel_api::abi::VERSION as u8;
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    bytes[16..24].copy_from_slice(&0x10000u64.to_le_bytes());
    bytes[24..32].copy_from_slice(&(2 * capsule::PAGE as u64).to_le_bytes());
    for (i, value) in [0x10000, 1, 1, capsule::PAGE as u64, 5]
        .into_iter()
        .enumerate()
    {
        bytes[capsule::HEADER + i * 8..capsule::HEADER + (i + 1) * 8]
            .copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn catalog(documents: &[d::v2::Unit<&'static str>]) -> Result<Catalog, Error> {
    let records: Vec<_> = documents.iter().map(record).collect();
    let units: Vec<_> = documents
        .iter()
        .zip(&records)
        .map(|(unit, record)| bundle::Unit {
            id: unit.id,
            record,
        })
        .collect();
    let files: Vec<_> = documents
        .iter()
        .map(|unit| bundle::File {
            kind: if unit.id == "system" {
                bundle::Kind::Supervisor
            } else {
                bundle::Kind::User
            },
            name: unit.loader.image,
            bytes: b"ELF",
        })
        .collect();
    let bytes = Box::leak(
        bundle::pack(&units, &files, 0, 0, &capsule())
            .unwrap()
            .into_boxed_slice(),
    );
    Catalog::decode(bundle::Archive::parse(bytes).unwrap())
}
fn record(unit: &d::v2::Unit<&str>) -> Vec<u8> {
    let owned: d::v2::Unit<String> = toml::from_str(&toml::to_string(unit).unwrap()).unwrap();
    unit_contract::record::v2::encode(&owned).unwrap()
}
fn system() -> d::v2::Unit<&'static str> {
    let mut unit = document("system");
    unit.loader.space = d::Space::Supervisor;
    unit
}

#[test]
fn native_rights_keep_kind_window_distribution_and_whole_nest_scope() {
    let mut unit = document("reader");
    let mut limit = rights(d::rights::Operations::Share(vec![
        d::rights::Share::Associate,
    ]));
    limit.window = d::rights::Window::Pages {
        offset: 1,
        count: 3,
    };
    limit.modifiable = true;
    limit.distributable = true;
    unit.supply.bindings.push(binding(limit));
    let native = Unit::compile(unit).unwrap();
    let limits = native.supply()[0].rights.0;
    assert_eq!(limits.operations, Operations::Share(ShareRights::ASSOCIATE));
    assert_eq!(
        limits.scope,
        PendScope::Pages(PendRange::pages(1, 3).unwrap())
    );
    assert_eq!(limits.modifiable, Modifiable::Yes);
    assert_eq!(limits.distributable, Distributable::Yes);
    let mut unit = document("nest");
    unit.supply
        .bindings
        .push(binding(rights(d::rights::Operations::Nest(vec![
            d::rights::Nest::Reserve,
        ]))));
    let limits = Unit::compile(unit).unwrap().supply()[0].rights.0;
    assert_eq!(limits.operations, Operations::Nest(NestRights::RESERVE));
    assert_eq!(limits.scope, PendScope::Nest(NestScope::Whole));
}

#[test]
fn invalid_permissions_and_kernel_identity_domains_fail_before_effects() {
    for bad in [
        d::Rights {
            modifiable: true,
            ..rights(d::rights::Operations::Sign(vec![d::rights::Sign::Trigger]))
        },
        d::Rights {
            window: d::rights::Window::Pages {
                offset: 0,
                count: 1,
            },
            ..rights(d::rights::Operations::Clone(vec![d::rights::Clone::Push]))
        },
        rights(d::rights::Operations::Share(vec![])),
        d::Rights {
            window: d::rights::Window::Pages {
                offset: u64::MAX,
                count: 1,
            },
            ..rights(d::rights::Operations::Share(vec![d::rights::Share::Open]))
        },
    ] {
        let mut unit = document("reader");
        unit.supply.bindings.push(binding(bad));
        assert!(matches!(Unit::compile(unit), Err(Error::Policy)));
    }
    for raw in [0, u64::MAX, isize::MAX as u64 + 1] {
        let mut unit = document("reader");
        unit.control.construction.push(d::Construction {
            unit: Some("reader"),
            owner: d::Owner::Task(raw),
            identity: d::IdentitySelection::Caller,
            arguments: d::Arguments::default(),
            stack: d::Range::default(),
            max_image: 0,
        });
        assert!(Unit::compile(unit).is_err());
    }
}

#[test]
fn selection_orders_startup_but_preserves_requested_units_without_starting_them() {
    let mut a = document("a");
    a.control.after = vec!["b", "optional"];
    let b = document("b");
    let mut requested = document("requested");
    requested.control.creation = d::Creation::Requested;
    a.control.requires.push("requested");
    let catalog = catalog(&[system(), a, b, requested]).unwrap();
    assert_eq!(
        catalog
            .startup()
            .unwrap()
            .iter()
            .map(|unit| unit.name())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert!(catalog.find("requested").is_some());
    assert_eq!(catalog.system(), "system");
}

#[test]
fn unresolved_authorities_and_readiness_cycles_are_rejected() {
    let mut a = document("a");
    a.control.requires.push("missing");
    assert!(matches!(catalog(&[system(), a]), Err(Error::Missing)));
    let mut a = document("a");
    a.control.after.push("b");
    let mut b = document("b");
    b.control.after.push("a");
    assert!(matches!(catalog(&[system(), a, b]), Err(Error::Cycle)));
    let mut a = document("a");
    a.control.retirement.push(d::Retirement {
        provider: "system",
        namespace: 0,
        face: "retired",
    });
    assert!(matches!(catalog(&[system(), a]), Err(Error::Policy)));
}

#[test]
fn record_decoding_keeps_boot_strings_borrowed_and_rejects_v1_records() {
    let bytes = Box::leak(record(&document("reader")).into_boxed_slice());
    let native = Unit::decode(bytes).unwrap();
    assert!(bytes.as_ptr_range().contains(&native.name().as_ptr()));
    assert!(
        bytes
            .as_ptr_range()
            .contains(&native.loader().image.as_ptr())
    );
    let old = d::Unit {
        schema: 1,
        id: "reader",
        loader: d::Loader {
            image: "reader",
            ..Default::default()
        },
        ..Default::default()
    };
    let owned: d::Unit<String> = toml::from_str(&toml::to_string(&old).unwrap()).unwrap();
    let bytes = Box::leak(
        unit_contract::record::encode(&owned)
            .unwrap()
            .into_boxed_slice(),
    );
    assert!(matches!(Unit::decode(bytes), Err(Error::Record)));
}

#[test]
fn retired_share_memo_and_unqualified_reservation_operations_fail_closed() {
    use d::rights::{Memo, Nest, Operations as O, Share};
    for operations in [
        O::Share(vec![Share::Open]),
        O::Share(vec![Share::Shut]),
        O::Memo(vec![Memo::Insert]),
        O::Nest(vec![Nest::Transfer]),
        O::Nest(vec![Nest::Publish]),
        O::Nest(vec![Nest::Release]),
    ] {
        let mut unit = document("reader");
        unit.supply.bindings.push(binding(rights(operations)));
        assert!(matches!(Unit::compile(unit), Err(Error::Policy)));
    }
}
