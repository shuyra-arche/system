#[path = "../../../server/src/system/loader/image/elf.rs"]
mod elf;

use elf::{Error, Plan, Target};

const ENTRY: u64 = 0x10000;
const HEADER: usize = 64;
const PROGRAM_HEADER: usize = 56;
const PAYLOAD: usize = HEADER + 2 * PROGRAM_HEADER;

fn parse(bytes: &[u8]) -> Result<Plan, Error> {
    elf::parse::<4096>(bytes, Target::RV64GC)
}

fn set(bytes: &mut [u8], offset: usize, value: &[u8]) {
    bytes[offset..offset + value.len()].copy_from_slice(value);
}

fn image() -> [u8; PAYLOAD + 4] {
    let mut bytes = [0; PAYLOAD + 4];
    set(&mut bytes, 0, b"\x7fELF\x02\x01\x01");
    set(&mut bytes, 16, &2u16.to_le_bytes());
    set(&mut bytes, 18, &243u16.to_le_bytes());
    set(&mut bytes, 20, &1u32.to_le_bytes());
    set(&mut bytes, 24, &ENTRY.to_le_bytes());
    set(&mut bytes, 32, &(HEADER as u64).to_le_bytes());
    set(&mut bytes, 52, &(HEADER as u16).to_le_bytes());
    set(&mut bytes, 54, &(PROGRAM_HEADER as u16).to_le_bytes());
    set(&mut bytes, 56, &1u16.to_le_bytes());
    set(&mut bytes, HEADER, &1u32.to_le_bytes());
    set(&mut bytes, HEADER + 4, &5u32.to_le_bytes());
    set(&mut bytes, HEADER + 8, &(PAYLOAD as u64).to_le_bytes());
    set(&mut bytes, HEADER + 16, &ENTRY.to_le_bytes());
    set(&mut bytes, HEADER + 32, &4u64.to_le_bytes());
    set(&mut bytes, HEADER + 40, &8u64.to_le_bytes());
    bytes
}

#[test]
fn admits_rv64_image_with_read_execute_permissions() {
    assert_eq!(
        Target::native().is_some(),
        cfg!(all(target_arch = "riscv64", target_feature = "c"))
    );
    let plan = parse(&image()).unwrap();
    assert_eq!(plan.entry, ENTRY as usize);
    let [region] = plan.regions.as_slice() else {
        panic!("expected one load region");
    };
    assert_eq!(region.va, ENTRY as usize);
    assert_eq!(region.flags, 2 | 8);
    assert_eq!(region.file_offset, PAYLOAD);
    assert_eq!(region.file_size, 4);
    assert_eq!(region.prefix, 0);
    assert_eq!(region.data_size, region.size);
}

#[test]
fn rejects_a_different_machine_and_invalid_entry_addresses() {
    let mut bytes = image();
    set(&mut bytes, 18, &183u16.to_le_bytes());
    assert_eq!(parse(&bytes).err(), Some(Error::Unsupported));

    let mut bytes = image();
    for offset in [1, 4, 6] {
        set(&mut bytes, 24, &(ENTRY + offset).to_le_bytes());
        assert_eq!(parse(&bytes).err(), Some(Error::Entry));
    }
    set(&mut bytes, 24, &(ENTRY + 2).to_le_bytes());
    assert!(parse(&bytes).is_ok());
}

#[test]
fn rejects_bad_segment_table_entries_and_overlapping_loads() {
    let mut bytes = image();
    assert_eq!(
        parse(&bytes[..HEADER + PROGRAM_HEADER - 1]).err(),
        Some(Error::Format)
    );
    for kind in [2u32, 3] {
        set(&mut bytes, HEADER, &kind.to_le_bytes());
        assert_eq!(parse(&bytes).err(), Some(Error::Unsupported));
    }

    let mut bytes = image();
    set(&mut bytes, HEADER + 4, &7u32.to_le_bytes());
    assert_eq!(parse(&bytes).err(), Some(Error::Permissions));

    let mut bytes = image();
    bytes.copy_within(HEADER..HEADER + PROGRAM_HEADER, HEADER + PROGRAM_HEADER);
    set(&mut bytes, 56, &2u16.to_le_bytes());
    assert_eq!(parse(&bytes).err(), Some(Error::Overlap));
}
