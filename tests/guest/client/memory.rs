//! Real Cell association/publication, Share ownership and target Memo cleanup.
use crate::{
    fixture::{self, Console, addr},
    loader_memory::Mapping,
};
use core::fmt::Write as _;
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{
            Allocation, CellCreate, CellObject, CellSource, CreatedCell, Placement, ReferenceRange,
        },
        identity::{PendId, TeamId},
        modifiable::Modifiable,
    },
};
use kernel_client::call::{cell, pend, room, share_view};
fn ready(mapping: &mut Mapping) {
    fixture::until(|| mapping.advance().unwrap());
    assert!(mapping.is_ready());
}
fn finish(mapping: &mut Mapping) {
    fixture::until(|| match mapping.finish() {
        Ok(()) => true,
        Err(error) if error.busy() => false,
        Err(error) => panic!("mapping cleanup {error:?}"),
    });
    mapping.finish().unwrap();
    assert!(mapping.address().is_none());
}
pub fn run(license: PendId) {
    let baseline = fixture::active();
    let request = CellCreate::Slot(Allocation::new(2 * PAGE_SIZE, PAGE_SIZE).unwrap()).to_bytes();
    let CreatedCell::Slot(slot) = cell::create(license, addr(&request)).unwrap() else {
        unreachable!()
    };
    let mut writer = Mapping::new(
        CellSource::Slot(ReferenceRange {
            reference: slot,
            offset: 0,
            count: 2,
        }),
        TeamId::new(0),
        Placement::lowest(PAGE_SIZE).unwrap(),
        6,
    )
    .unwrap();
    assert!(writer.address().is_none());
    ready(&mut writer);
    let address = writer.address().unwrap().get();
    // SAFETY: this Task exclusively owns the published private writable span.
    unsafe {
        core::ptr::write_bytes(address as *mut u8, 0xa5, 2 * PAGE_SIZE);
    }
    let share = share_view::create(license, slot, Modifiable::No).unwrap();
    finish(&mut writer);
    cell::delete(addr(&CellObject::Slot(slot).to_bytes())).unwrap();
    let mut reader = Mapping::new(
        CellSource::Pend {
            token: share,
            offset: 0,
            count: 2,
        },
        TeamId::new(0),
        Placement::lowest(PAGE_SIZE).unwrap(),
        2,
    )
    .unwrap();
    ready(&mut reader);
    let address = reader.address().unwrap().get();
    // SAFETY: the owned Share root and Memo keep this immutable span alive.
    assert!(
        unsafe { core::slice::from_raw_parts(address as *const u8, 2 * PAGE_SIZE) }
            .iter()
            .all(|byte| *byte == 0xa5)
    );
    finish(&mut reader);
    pend::return_(share).unwrap();
    assert_eq!(fixture::active(), baseline);

    let request = CellCreate::Slot(Allocation::new(PAGE_SIZE, PAGE_SIZE).unwrap()).to_bytes();
    let CreatedCell::Slot(slot) = cell::create(license, addr(&request)).unwrap() else {
        unreachable!()
    };
    let mut partial = Mapping::new(
        CellSource::Slot(ReferenceRange {
            reference: slot,
            offset: 0,
            count: 1,
        }),
        TeamId::new(0),
        Placement::lowest(PAGE_SIZE).unwrap(),
        6,
    )
    .unwrap();
    assert!(!partial.advance().unwrap());
    finish(&mut partial);
    cell::delete(addr(&CellObject::Slot(slot).to_bytes())).unwrap();
    assert_eq!(fixture::active(), baseline);
    assert!(
        Mapping::new(
            CellSource::Pend {
                token: license,
                offset: 0,
                count: 1
            },
            TeamId::new(0),
            Placement::exact(VirtAddr::new(0x10000), PAGE_SIZE).unwrap(),
            14
        )
        .is_err()
    );
    room::yield_();
    writeln!(
        Console,
        "PASS loader-memory native=1 insert=1 publish=1 share=1 partial=1 cleanup=1"
    )
    .unwrap();
}
