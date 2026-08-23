#[derive(Clone, Debug)]
pub(super) struct BlockBuffer<const BLOCK_SIZE: usize> {
    bytes: [u8; BLOCK_SIZE],
    len: usize,
}

impl<const BLOCK_SIZE: usize> BlockBuffer<BLOCK_SIZE> {
    pub(super) const fn new() -> Self {
        Self {
            bytes: [0; BLOCK_SIZE],
            len: 0,
        }
    }

    pub(super) fn consume(&mut self, mut input: &[u8], mut consume: impl FnMut(&[u8])) {
        debug_assert!(BLOCK_SIZE.is_power_of_two());
        if self.len != 0 {
            let needed = BLOCK_SIZE - self.len;
            let copied = needed.min(input.len());
            self.bytes[self.len..self.len + copied].copy_from_slice(&input[..copied]);
            self.len += copied;
            input = &input[copied..];
            if self.len != BLOCK_SIZE {
                return;
            }
            consume(&self.bytes);
            self.len = 0;
        }

        let body_len = input.len() & !(BLOCK_SIZE - 1);
        if body_len != 0 {
            consume(&input[..body_len]);
        }
        let remaining = &input[body_len..];
        self.bytes[..remaining.len()].copy_from_slice(remaining);
        self.len = remaining.len();
    }

    pub(super) fn remaining(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
