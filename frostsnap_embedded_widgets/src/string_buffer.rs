use core::fmt::{self, Display, Write};

/// A simple string buffer that doesn't allocate
/// Useful for formatting strings in no_std environments without allocations
#[derive(Clone, Copy)]
pub struct StringBuffer<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> StringBuffer<N> {
    pub fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<const N: usize> Write for StringBuffer<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = N - self.len;
        let to_write = bytes.len().min(remaining);
        self.buf[self.len..self.len + to_write].copy_from_slice(&bytes[..to_write]);
        self.len += to_write;
        Ok(())
    }
}

impl<const N: usize> Default for StringBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Display for StringBuffer<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const N: usize> AsRef<str> for StringBuffer<N> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> PartialEq for StringBuffer<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const N: usize> Eq for StringBuffer<N> {}

impl<const N: usize> PartialEq<str> for StringBuffer<N> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<const N: usize> PartialEq<&str> for StringBuffer<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<const N: usize> fmt::Debug for StringBuffer<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StringBuffer(\"{}\")", self.as_str())
    }
}
