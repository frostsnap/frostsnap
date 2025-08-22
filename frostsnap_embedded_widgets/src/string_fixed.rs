use core::fmt::{self, Display, Write};

/// A fixed-size string that doesn't allocate
/// Useful for formatting strings in no_std environments without allocations
#[derive(Clone, Copy)]
pub struct StringFixed<const N: usize> {
    buf: [u8; N],
    len: usize,
    characters_per_row: usize,
    last_newline: usize,
}

impl<const N: usize> StringFixed<N> {
    pub fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
            characters_per_row: usize::MAX,
            last_newline: 0,
        }
    }

    pub fn with_wrap(characters_per_row: usize) -> Self {
        Self {
            buf: [0; N],
            len: 0,
            characters_per_row,
            last_newline: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let mut buffer = Self::new();
        let _ = buffer.write_str(s);
        buffer
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.last_newline = 0;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn add_char(&mut self, ch: u8) -> Option<()> {
        if (self.last_newline % self.characters_per_row) == 0 && self.last_newline > 0 {
            *self.buf.get_mut(self.len)? = b'\n';
            self.len += 1;
        }

        if self.len > 0 && self.buf[self.len - 1] == b'\n' {
            self.last_newline = 0;
        }

        *self.buf.get_mut(self.len)? = ch;
        self.len += 1;
        self.last_newline += 1;
        Some(())
    }
}

impl<const N: usize> Write for StringFixed<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.as_bytes() {
            if self.add_char(*byte).is_none() {
                break;
            }
        }
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        if let Ok(ch) = c.try_into() {
            self.add_char(ch);
        } else {
            self.add_char(b'?');
        }
        Ok(())
    }
}

impl<const N: usize> Default for StringFixed<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Display for StringFixed<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const N: usize> AsRef<str> for StringFixed<N> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> PartialEq for StringFixed<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const N: usize> Eq for StringFixed<N> {}

impl<const N: usize> PartialEq<str> for StringFixed<N> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<const N: usize> PartialEq<&str> for StringFixed<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<const N: usize> fmt::Debug for StringFixed<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StringFixed(\"{}\")", self.as_str())
    }
}
