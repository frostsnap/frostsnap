use alloc::string::String;
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

    pub fn from_string(s: &str) -> Self {
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
        if self.last_newline >= self.characters_per_row && self.characters_per_row > 0 {
            *self.buf.get_mut(self.len)? = b'\n';
            self.len += 1;
            self.last_newline = 0;
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

/// A dynamic string with line wrapping support
/// Unlike StringFixed, this allocates and can grow as needed
#[derive(Clone)]
pub struct StringWrap {
    buf: String,
    characters_per_row: usize,
    last_newline: usize,
}

impl Default for StringWrap {
    fn default() -> Self {
        Self::new()
    }
}

impl StringWrap {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            characters_per_row: usize::MAX,
            last_newline: 0,
        }
    }

    pub fn with_wrap(characters_per_row: usize) -> Self {
        Self {
            buf: String::new(),
            characters_per_row,
            last_newline: 0,
        }
    }

    pub fn from_str(s: &str, characters_per_row: usize) -> Self {
        let mut wrapped = Self::with_wrap(characters_per_row);
        let _ = wrapped.write_str(s);
        wrapped
    }

    pub fn as_str(&self) -> &str {
        &self.buf
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.last_newline = 0;
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    fn add_char(&mut self, ch: char) {
        // Add newline if we've reached the character limit for this row
        if self.last_newline >= self.characters_per_row && self.characters_per_row > 0 {
            self.buf.push('\n');
            self.last_newline = 0;
        }

        // Reset newline counter if we just added a newline
        if !self.buf.is_empty() && self.buf.ends_with('\n') {
            self.last_newline = 0;
        }

        self.buf.push(ch);
        self.last_newline += 1;
    }
}

impl Write for StringWrap {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for ch in s.chars() {
            self.add_char(ch);
        }
        Ok(())
    }
}

impl Display for StringWrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for StringWrap {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<StringWrap> for String {
    fn from(wrapped: StringWrap) -> Self {
        wrapped.buf
    }
}

impl From<&StringWrap> for String {
    fn from(wrapped: &StringWrap) -> Self {
        wrapped.buf.clone()
    }
}

impl<const N: usize> fmt::Debug for StringFixed<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StringFixed(\"{}\")", self.as_str())
    }
}
