use env::TaskId;
use env::wire::Span as _;
use system_api::{bootstrap::Frame, supply::{MAX_DESCRIPTION, SupplyFrame}};
use weave::{Codec, EncodedSize};
use wire::Message;

#[test]
fn bootstrap_encoding_preserves_legacy_frame() {
    let frame = Frame {
        version: 1,
        role: 2,
        provider: TaskId::new(0x1234),
        system: TaskId::new(0x5678),
    };
    assert_eq!(<Frame as Codec>::SIZE, EncodedSize::Fixed(Frame::LEN));
    let mut bytes = [0u8; Frame::LEN];
    assert_eq!(frame.encode(&mut bytes), Some(Frame::LEN));
    let mut legacy = [0u8; Frame::LEN];
    assert_eq!(frame.store_at(&mut legacy, 0), Some(Frame::LEN));
    assert_eq!(legacy, bytes);
    assert_eq!(<Frame as Message>::store(&frame, &mut legacy), Some(Frame::LEN));
    assert_eq!(legacy, bytes);

    let decoded = Frame::decode_exact(&bytes).unwrap();
    assert_eq!(decoded, frame);
    assert_eq!(<Frame as Message>::fetch(&bytes), Some(frame));
    assert!(Frame::decode_exact(&bytes[..Frame::LEN - 1]).is_none());
    let mut trailing = [0u8; Frame::LEN + 1];
    trailing[..Frame::LEN].copy_from_slice(&bytes);
    assert!(Frame::decode_exact(&trailing).is_none());
}

#[test]
fn supply_encoding_checks_length_and_preserves_wire_bytes() {
    let mut description = [0u8; MAX_DESCRIPTION];
    description[..3].copy_from_slice(b"abc");
    let frame = SupplyFrame { length: 3, description };
    assert_eq!(<SupplyFrame as Codec>::SIZE, EncodedSize::Fixed(SupplyFrame::LEN));
    let mut bytes = [0u8; SupplyFrame::LEN];
    assert_eq!(frame.encode(&mut bytes), Some(SupplyFrame::LEN));
    assert_eq!(&bytes[..5], &[3, 0, b'a', b'b', b'c']);
    let mut legacy = [0u8; SupplyFrame::LEN];
    assert_eq!(<SupplyFrame as Message>::store(&frame, &mut legacy), Some(SupplyFrame::LEN));
    assert_eq!(legacy, bytes);

    let decoded = SupplyFrame::decode_exact(&bytes).unwrap();
    assert_eq!(decoded.length, frame.length);
    assert_eq!(decoded.description, frame.description);
    assert!(SupplyFrame::decode_exact(&bytes[..SupplyFrame::LEN - 1]).is_none());
    let mut trailing = [0u8; SupplyFrame::LEN + 1];
    trailing[..SupplyFrame::LEN].copy_from_slice(&bytes);
    assert!(SupplyFrame::decode_exact(&trailing).is_none());
    assert!(<SupplyFrame as Message>::store(&frame, &mut trailing).is_none());

    let invalid = SupplyFrame { length: MAX_DESCRIPTION as u16 + 1, description };
    assert!(invalid.encode(&mut bytes).is_none());
    bytes[..2].copy_from_slice(&invalid.length.to_le_bytes());
    assert!(SupplyFrame::decode_exact(&bytes).is_none());
}
