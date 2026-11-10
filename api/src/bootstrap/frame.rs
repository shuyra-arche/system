use env::TaskId;
use env::wire::Span as _;
use wire::Message;

#[derive(env::Frame, Clone, Copy, PartialEq, Eq, Debug)]
#[frame(len = 18)]
pub struct Frame {
    pub version: u8,
    pub role: u8,
    pub provider: TaskId,
    pub system: TaskId,
}

const _: () = assert!(Frame::LEN == 2 + <TaskId as env::wire::Field>::WIDTH * 2);

// The Weave prefix codec owns the byte layout. Keep the old Span derive only
// while consumers still require its public API and the legacy framing contract.
impl weave::Codec for Frame {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = weave::EncodedSize::Fixed(Self::LEN);

    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        let out = out.get_mut(..Self::LEN)?;
        out[0] = self.version;
        out[1] = self.role;
        out[2..10].copy_from_slice(&u64::try_from(self.provider.get()).ok()?.to_le_bytes());
        out[10..18].copy_from_slice(&u64::try_from(self.system.get()).ok()?.to_le_bytes());
        Some(Self::LEN)
    }

    fn decode(bytes: &[u8]) -> Option<(Self::Decoded, usize)> {
        let bytes = bytes.get(..Self::LEN)?;
        let provider = usize::try_from(u64::from_le_bytes(bytes[2..10].try_into().ok()?)).ok()?;
        let system = usize::try_from(u64::from_le_bytes(bytes[10..18].try_into().ok()?)).ok()?;
        Some((
            Self {
                version: bytes[0],
                role: bytes[1],
                provider: TaskId::new(provider),
                system: TaskId::new(system),
            },
            Self::LEN,
        ))
    }
}

impl Message for Frame {
    type In = Self;
    type Buf = [u8; Self::LEN];
    const EMPTY: Self::Buf = [0; Self::LEN];

    fn store(&self, out: &mut [u8]) -> Option<usize> {
        <Self as weave::Codec>::encode(self, out)
    }

    fn fetch(bytes: &[u8]) -> Option<Self> {
        <Self as weave::Codec>::decode_exact(bytes)
    }
}
