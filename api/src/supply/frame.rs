use super::MAX_DESCRIPTION;

/// Bounded metadata delivered beside a resource already held by its recipient.
#[derive(Clone, Copy)]
pub struct SupplyFrame {
    pub length: u16,
    pub description: [u8; MAX_DESCRIPTION],
}

impl SupplyFrame {
    pub const LEN: usize = 2 + MAX_DESCRIPTION;
}

impl weave::Codec for SupplyFrame {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = weave::EncodedSize::Fixed(Self::LEN);

    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if usize::from(self.length) > self.description.len() {
            return None;
        }
        let out = out.get_mut(..Self::LEN)?;
        out[..2].copy_from_slice(&self.length.to_le_bytes());
        out[2..].copy_from_slice(&self.description);
        Some(Self::LEN)
    }

    fn decode(bytes: &[u8]) -> Option<(Self::Decoded, usize)> {
        let bytes = bytes.get(..Self::LEN)?;
        let length = u16::from_le_bytes(bytes[..2].try_into().ok()?);
        if usize::from(length) > MAX_DESCRIPTION {
            return None;
        }
        let mut description = [0; MAX_DESCRIPTION];
        description.copy_from_slice(&bytes[2..]);
        Some((Self { length, description }, Self::LEN))
    }
}

// Preserve the existing exact-buffer rule until service transports move to Weave.
impl wire::Message for SupplyFrame {
    type In = Self;
    type Buf = [u8; Self::LEN];
    const EMPTY: Self::Buf = [0; Self::LEN];

    fn store(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() != Self::LEN {
            return None;
        }
        <Self as weave::Codec>::encode(self, out)
    }

    fn fetch(bytes: &[u8]) -> Option<Self> {
        <Self as weave::Codec>::decode_exact(bytes)
    }
}
