//! Fixed-width wire identities, bounded names and the versioned request envelope.
use alloc::string::String;
use weave::{Code, Codec, Coded, EncodedSize, codec};

macro_rules! identifier {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, weave::Frame)]
        pub struct $name {
            value: u64,
        }
        impl $name {
            pub const NONE: Self = Self { value: 0 };
            pub const fn new(value: u64) -> Self { Self { value } }
            pub const fn get(self) -> u64 { self.value }
        }
    )*};
}
identifier!(TaskId, TeamId, PendId, Mark, EntryId);

/// A declaration name is one nonempty path component, bounded in UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(String);

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 31
        && !matches!(name, "." | "..")
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

impl Name {
    pub fn new(value: &str) -> Option<Self> {
        valid_name(value).then(|| Self(String::from(value)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Codec for Name {
    type Decoded = Self;
    const SIZE: EncodedSize = EncodedSize::Bounded(32);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        codec::bounded::encode_text::<u8>(&self.0, 31, out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (value, used) = codec::bounded::decode_owned_text::<u8>(bytes, 31)?;
        valid_name(&value).then_some((Self(value), used))
    }
}

/// Canonical relative path, including the empty root, with at most eight names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path(String);
impl Path {
    fn valid(value: &str) -> bool {
        value.is_empty()
            || (value.len() <= 255
                && value.split('/').count() <= 8
                && value.split('/').all(valid_name))
    }
    pub fn new(value: &str) -> Option<Self> {
        Self::valid(value).then(|| Self(String::from(value)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Codec for Path {
    type Decoded = Self;
    const SIZE: EncodedSize = EncodedSize::Bounded(256);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        codec::bounded::encode_text::<u8>(&self.0, 255, out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (value, used) = codec::bounded::decode_owned_text::<u8>(bytes, 255)?;
        Self::valid(&value).then_some((Self(value), used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Optional<T>(pub Option<T>);
impl<T: Codec<Decoded = T>> Codec for Optional<T> {
    type Decoded = Self;
    const SIZE: EncodedSize = EncodedSize::sum(&[
        EncodedSize::Fixed(1),
        EncodedSize::choice(&[EncodedSize::Fixed(0), T::SIZE]),
    ]);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        let Some(value) = &self.0 else {
            return false.encode(out);
        };
        let used = true.encode(out)?;
        Some(used + codec::encode(value, out.get_mut(used..)?)?)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (present, used) = bool::decode(bytes)?;
        if !present {
            return Some((Self(None), used));
        }
        let (value, length) = codec::decode::<T>(bytes.get(used..)?)?;
        Some((Self(Some(value)), used + length))
    }
}

/// Success and known protocol failure have disjoint tagged representations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<T, E> {
    Success(T),
    Failure(E),
}
impl<T: Codec<Decoded = T>, E: Code<Repr = u8>> Codec for Outcome<T, E> {
    type Decoded = Self;
    const SIZE: EncodedSize = EncodedSize::sum(&[
        EncodedSize::Fixed(1),
        EncodedSize::choice(&[T::SIZE, EncodedSize::Fixed(1)]),
    ]);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        match self {
            Self::Success(value) => {
                let used = 0u8.encode(out)?;
                Some(used + codec::encode(value, out.get_mut(used..)?)?)
            }
            Self::Failure(error) => {
                let used = 1u8.encode(out)?;
                Some(used + error.code().encode(out.get_mut(used..)?)?)
            }
        }
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (tag, used) = u8::decode(bytes)?;
        match tag {
            0 => {
                let (value, n) = codec::decode::<T>(bytes.get(used..)?)?;
                Some((Self::Success(value), used + n))
            }
            1 => {
                let (error, n) = Coded::<E>::decode(bytes.get(used..)?)?;
                Some((Self::Failure(error.0), used + n))
            }
            _ => None,
        }
    }
}

#[derive(weave::Frame)]
struct Header {
    magic: u32,
    version: u16,
    service: u16,
    reply: PendId,
}

/// One-shot reply routing. Transport must authenticate the sender separately.
pub struct Request<T, const SERVICE: u16> {
    pub reply: PendId,
    pub body: T,
}
impl<T: Codec, const SERVICE: u16> Codec for Request<T, SERVICE> {
    type Decoded = Request<T::Decoded, SERVICE>;
    const SIZE: EncodedSize = EncodedSize::sum(&[Header::SIZE, T::SIZE]);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if self.reply == PendId::NONE {
            return None;
        }
        let used = codec::encode(
            &Header {
                magic: 0x32535953,
                version: crate::VERSION,
                service: SERVICE,
                reply: self.reply,
            },
            out,
        )?;
        Some(used + codec::encode(&self.body, out.get_mut(used..)?)?)
    }
    fn decode(bytes: &[u8]) -> Option<(Self::Decoded, usize)> {
        let (header, used) = codec::decode::<Header>(bytes)?;
        if header.magic != 0x32535953
            || header.version != crate::VERSION
            || header.service != SERVICE
            || header.reply == PendId::NONE
        {
            return None;
        }
        let (body, n) = codec::decode::<T>(bytes.get(used..)?)?;
        Some((
            Request {
                reply: header.reply,
                body,
            },
            used + n,
        ))
    }
}

/// Kernel v10 authority is encoded through the provider's checked word contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rights(pub kernel_api::model::pend::Limits);
impl Codec for Rights {
    type Decoded = Self;
    const SIZE: EncodedSize = EncodedSize::Fixed(72);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.0.valid() {
            return None;
        }
        self.0.words().map(|word| word as u64).encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (words, used) = <[u64; 9]>::decode(bytes)?;
        let mut native = [0; 9];
        for (target, word) in native.iter_mut().zip(words) {
            *target = usize::try_from(word).ok()?;
        }
        Some((Self(kernel_api::model::pend::Limits::of(native)?), used))
    }
}

// Native conversion is checked at the ABI boundary. The wire keeps fixed-width IDs.
macro_rules! native_id {
    ($name:ident) => {
        impl From<kernel_api::model::identity::$name> for $name {
            fn from(value: kernel_api::model::identity::$name) -> Self {
                Self::new(value.get() as u64)
            }
        }
        impl TryFrom<$name> for kernel_api::model::identity::$name {
            type Error = ();
            fn try_from(value: $name) -> Result<Self, ()> {
                let raw = usize::try_from(value.get()).map_err(|_| ())?;
                if raw > isize::MAX as usize {
                    return Err(());
                }
                Ok(Self::new(raw))
            }
        }
    };
}
native_id!(TaskId);
native_id!(TeamId);
impl From<kernel_api::model::identity::PendId> for PendId {
    fn from(value: kernel_api::model::identity::PendId) -> Self {
        Self::new(value.get() as u64)
    }
}
impl TryFrom<PendId> for kernel_api::model::identity::PendId {
    type Error = ();
    fn try_from(value: PendId) -> Result<Self, ()> {
        if value.get() > isize::MAX as u64 {
            return Err(());
        }
        Self::from_bytes(&value.get().to_le_bytes()).ok_or(())
    }
}
impl From<kernel_api::model::identity::Mark> for Mark {
    fn from(value: kernel_api::model::identity::Mark) -> Self {
        Self::new(value.get())
    }
}
impl From<Mark> for kernel_api::model::identity::Mark {
    fn from(value: Mark) -> Self {
        Self::new(value.get())
    }
}
