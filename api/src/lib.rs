#![no_std]
//! System protocol v2. Received identities come from transport metadata.
//! A serialized Pend is local to the recipient and must be granted before send.
extern crate alloc;

// Sum payloads use Weave's checked child codecs and explicit numeric tags.
macro_rules! union_codec {
    ($name:ident { $($unit:ident = $unit_code:literal),* ; $($variant:ident($ty:ty) = $code:literal),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $name { $($unit,)* $($variant($ty),)* }
        impl ::weave::Codec for $name {
            type Decoded = Self;
            const SIZE: ::weave::EncodedSize = ::weave::EncodedSize::sum(&[
                ::weave::EncodedSize::Fixed(1),
                ::weave::EncodedSize::choice(&[
                    $( { let _ = $unit_code; ::weave::EncodedSize::Fixed(0) }, )*
                    $( <$ty as ::weave::Codec>::SIZE, )*
                ]),
            ]);
            fn encode(&self, out: &mut [u8]) -> Option<usize> {
                match self {
                    $( Self::$unit => ::weave::codec::encode(&($unit_code as u8), out), )*
                    $( Self::$variant(value) => {
                        let used = ::weave::codec::encode(&($code as u8), out)?;
                        Some(used + ::weave::codec::encode(value, out.get_mut(used..)?)?)
                    }, )*
                }
            }
            fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
                let (tag, used) = ::weave::codec::decode::<u8>(bytes)?;
                match tag {
                    $( $unit_code => Some((Self::$unit, used)), )*
                    $( $code => {
                        let (value, n) = ::weave::codec::decode::<$ty>(bytes.get(used..)?)?;
                        Some((Self::$variant(value), used + n))
                    }, )*
                    _ => None,
                }
            }
        }
    };
}

// Transaction identities reserve zero and never wrap into another operation.
macro_rules! request_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(core::num::NonZeroU64);
        impl $name {
            pub const fn new(value: u64) -> Option<Self> {
                match core::num::NonZeroU64::new(value) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }
            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
        impl ::weave::Codec for $name {
            type Decoded = Self;
            const SIZE: ::weave::EncodedSize = ::weave::EncodedSize::Fixed(8);
            fn encode(&self, out: &mut [u8]) -> Option<usize> {
                ::weave::codec::encode(&self.get(), out)
            }
            fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
                let (value, used) = ::weave::codec::decode::<u64>(bytes)?;
                Some((Self::new(value)?, used))
            }
        }
    };
}

pub mod bootstrap;
pub mod control;
pub mod identity;
pub mod loader;
pub mod namespace;
pub mod operator;
pub mod retirement;
pub mod supply;
pub mod value;

pub use value::{EntryId, Mark, Name, Path, PendId, TaskId, TeamId};
pub const VERSION: u16 = 2;
