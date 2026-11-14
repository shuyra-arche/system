use kernel_api::{
    boot::{
        capsule,
        ledger::{self, Call, Entry, Name, Page, ResourceKind, Trap},
    },
    model::{unit::ControlId, view::ViewKind},
};
use system_support::boot::{Accounts, Catalog, Resource};
use unit_contract::bundle::{self, File, Kind, Unit};

fn image() -> Vec<u8> {
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
    assert!(capsule::Capsule::parse(&bytes).is_some());
    bytes
}
fn bundle(boot: &[u8]) -> Vec<u8> {
    bundle::pack(
        &[Unit {
            id: "system",
            record: b"opaque record",
        }],
        &[File {
            kind: Kind::Supervisor,
            name: "system",
            bytes: b"ELF bytes",
        }],
        0,
        0,
        boot,
    )
    .unwrap()
}
fn ledger(entries: &[Entry]) -> Vec<u8> {
    let mut bytes = vec![0; ledger::Header::LEN + entries.len() * Entry::LEN];
    ledger::store_entries(&mut bytes, entries).unwrap();
    bytes
}

#[test]
fn startup_preserves_control_permission_and_paired_data_event_domains() {
    let bytes = bundle(&image());
    let record = ledger(&[
        Entry::new(Name::Call(Call::Build), ResourceKind::Control, 41),
        Entry::new(Name::Call(Call::Create), ResourceKind::None, 41),
        Entry::new(Name::Page(Page::Dtb), ResourceKind::Share, 42),
        Entry::pair(Name::Memory, ViewKind::Share, 43, 44),
        Entry::pair(
            Name::Interrupt {
                controller: 0xc000000,
                source: 10,
            },
            ViewKind::Share,
            45,
            46,
        ),
        Entry::new(Name::Trap(Trap::PageFault), ResourceKind::Clone, 47),
        Entry::new(
            Name::Nest {
                controller: 0x3010000,
                requester: 0,
            },
            ResourceKind::Nest,
            48,
        ),
    ]);
    let accounts = Accounts::parse(&bytes, &record).unwrap();
    assert_eq!(accounts.control(Call::Build), Some(ControlId::new(41)));
    assert_eq!(accounts.permission(Call::Create).unwrap().get(), 41);
    assert_eq!(accounts.control(Call::Create), None);
    assert_eq!(accounts.permission(Call::Build), None);
    assert_eq!(accounts.page(Page::Dtb).unwrap().get(), 42);
    let pair = accounts.dispatch(Name::Memory).unwrap();
    assert_eq!(
        (pair.view.0, pair.view.1.get(), pair.sign.get()),
        (ViewKind::Share, 43, 44)
    );
    assert!(
        matches!(accounts.resource(Name::Trap(Trap::PageFault)), Some(Resource::View(ViewKind::Clone, id)) if id.get() == 47)
    );
    assert_eq!(accounts.entries().count(), 7);
    assert_eq!(
        accounts.page(Page::Memory),
        None,
        "observation pair is not an interchangeable page token"
    );
}

#[test]
fn malformed_or_old_ledgers_fail_without_exposing_partial_boot_state() {
    let bytes = bundle(&image());
    let entry = Entry::new(Name::Call(Call::Create), ResourceKind::None, 1);
    for entries in [
        vec![entry, entry],
        vec![Entry::new(Name::Memory, ResourceKind::Sign, 3)],
        vec![Entry::new(Name::Call(Call::Build), ResourceKind::None, 4)],
        vec![Entry::pair(Name::Memory, ViewKind::Share, 7, 7)],
    ] {
        assert!(Accounts::parse(&bytes, &ledger(&entries)).is_none());
    }
    let record = ledger(&[entry]);
    for end in 0..record.len() {
        assert!(Accounts::parse(&bytes, &record[..end]).is_none());
    }
    let mut stale = record.clone();
    stale[..4].copy_from_slice(&8u32.to_le_bytes());
    assert!(Accounts::parse(&bytes, &stale).is_none());
    let mut extra = record;
    extra.push(0);
    assert!(Accounts::parse(&bytes, &extra).is_none());
}

#[test]
fn trusted_four_word_startup_borrows_exact_kernel_mappings() {
    let bytes = bundle(&image());
    let record = ledger(&[]);
    let words = [
        bytes.as_ptr().addr(),
        bytes.len(),
        record.as_ptr().addr(),
        record.len(),
    ];
    // SAFETY: these allocations remain immutable and live through every borrow.
    let accounts = unsafe { Accounts::from_words(&words) }.unwrap();
    assert_eq!(accounts.view().as_ptr(), bytes.as_ptr());
    assert_eq!(
        Catalog::of_boot(&accounts).unwrap().entry().bytes,
        b"ELF bytes"
    );
    for invalid in [
        words[..3].to_vec(),
        [words.as_slice(), &[0]].concat(),
        vec![0, 1, words[2], words[3]],
        vec![usize::MAX, 2, words[2], words[3]],
        vec![words[0], 0, words[2], words[3]],
    ] {
        // Invalid framing/ranges must return before dereferencing any address.
        assert!(unsafe { Accounts::from_words(&invalid) }.is_none());
    }
}

#[test]
fn opaque_unit_payload_still_requires_a_current_kernel_capsule_at_system_boundary() {
    let record = ledger(&[]);
    let mut boot = image();
    boot[11] = 8;
    let bytes = bundle(&boot);
    assert!(
        bundle::Archive::parse(&bytes).is_ok(),
        "Unit does not choose Kernel ABI"
    );
    assert!(Catalog::new(&bytes).is_none());
    assert!(Accounts::parse(&bytes, &record).is_none());
    let bytes = bundle(&image());
    for end in 0..bytes.len() {
        assert!(Catalog::new(&bytes[..end]).is_none());
    }
}
