/// A fixed length string that doesn't require allocations.
///
/// Useful in panic handlers. If more text is written than it can fit it just silently overflows.
pub struct PanicBuffer<const N: usize> {
    buffer: [u8; N],
    buf_len: usize,
}

impl<const N: usize> Default for PanicBuffer<N> {
    fn default() -> Self {
        Self {
            buffer: [0u8; N],
            buf_len: 0,
        }
    }
}

impl<const N: usize> PanicBuffer<N> {
    pub fn as_str(&self) -> &str {
        match core::str::from_utf8(&self.buffer[..self.buf_len]) {
            Ok(string) => string,
            Err(_) => "failed to render panic",
        }
    }

    fn head(&mut self) -> &mut [u8] {
        &mut self.buffer[self.buf_len..]
    }
}

impl<const N: usize> core::fmt::Write for PanicBuffer<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let len = self.head().len().min(s.len());
        self.head()[..len].copy_from_slice(&s.as_bytes()[..len]);
        self.buf_len += len;
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        let head = self.head();
        if !head.is_empty() {
            head[0] = c as u8;
            self.buf_len += 1;
        }
        Ok(())
    }
}
