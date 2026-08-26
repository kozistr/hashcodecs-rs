#[derive(Clone, Copy, Debug)]
pub(super) struct FullBlocks<'a, const BLOCK_SIZE: usize>(&'a [u8]);

impl<'a, const BLOCK_SIZE: usize> FullBlocks<'a, BLOCK_SIZE> {
    #[cfg(any(test, kani, target_arch = "x86", target_arch = "x86_64"))]
    #[inline]
    pub(super) fn new(bytes: &'a [u8]) -> Option<Self> {
        (BLOCK_SIZE != 0 && bytes.len().is_multiple_of(BLOCK_SIZE)).then_some(Self(bytes))
    }

    #[inline]
    pub(super) fn split(input: &'a [u8]) -> (Self, &'a [u8]) {
        assert!(BLOCK_SIZE != 0, "block size must not be zero");
        let body_len = input.len() / BLOCK_SIZE * BLOCK_SIZE;
        (Self(&input[..body_len]), &input[body_len..])
    }

    #[inline(always)]
    pub(super) fn as_bytes(self) -> &'a [u8] {
        self.0
    }

    #[inline(always)]
    pub(super) fn len(self) -> usize {
        self.0.len()
    }
}

#[derive(Clone, Debug)]
pub(super) struct BlockBuffer<const BLOCK_SIZE: usize> {
    bytes: [u8; BLOCK_SIZE],
    len: usize,
}

impl<const BLOCK_SIZE: usize> BlockBuffer<BLOCK_SIZE> {
    pub(super) const fn new() -> Self {
        assert!(BLOCK_SIZE != 0, "block size must not be zero");
        Self {
            bytes: [0; BLOCK_SIZE],
            len: 0,
        }
    }

    pub(super) fn consume(
        &mut self,
        mut input: &[u8],
        mut consume: impl FnMut(FullBlocks<'_, BLOCK_SIZE>),
    ) {
        if self.len != 0 {
            let needed = BLOCK_SIZE - self.len;
            let copied = needed.min(input.len());
            self.bytes[self.len..self.len + copied].copy_from_slice(&input[..copied]);
            self.len += copied;
            input = &input[copied..];
            if self.len != BLOCK_SIZE {
                return;
            }
            consume(FullBlocks(&self.bytes));
            self.len = 0;
        }

        let (blocks, remaining) = FullBlocks::split(input);
        if blocks.len() != 0 {
            consume(blocks);
        }
        self.bytes[..remaining.len()].copy_from_slice(remaining);
        self.len = remaining.len();
    }

    pub(super) fn remaining(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
