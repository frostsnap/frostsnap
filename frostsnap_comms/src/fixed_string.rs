use alloc::string::{String, ToString};
use core::fmt;
use core::ops::Deref;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FixedString<const N: usize> {
    inner: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringTooLong {
    pub max_len: usize,
    pub actual_len: usize,
}

impl fmt::Display for StringTooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "String too long: max length is {} but got {}",
            self.max_len, self.actual_len
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StringTooLong {}

impl<const N: usize> FixedString<N> {
    pub fn new(s: String) -> Result<Self, StringTooLong> {
        if s.len() > N {
            Err(StringTooLong {
                max_len: N,
                actual_len: s.len(),
            })
        } else {
            Ok(Self { inner: s })
        }
    }

    pub fn from_str(s: &str) -> Result<Self, StringTooLong> {
        Self::new(s.to_string())
    }

    pub fn truncate(mut s: String) -> Self {
        if s.len() > N {
            s.truncate(N);
        }
        Self { inner: s }
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn into_string(self) -> String {
        self.inner
    }

    pub fn max_length() -> usize {
        N
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<const N: usize> Deref for FixedString<N> {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<const N: usize> AsRef<str> for FixedString<N> {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl<const N: usize> fmt::Display for FixedString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl<const N: usize> TryFrom<String> for FixedString<N> {
    type Error = StringTooLong;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const N: usize> TryFrom<&str> for FixedString<N> {
    type Error = StringTooLong;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl<const N: usize> Default for FixedString<N> {
    fn default() -> Self {
        Self {
            inner: String::new(),
        }
    }
}

impl<const N: usize> bincode::Encode for FixedString<N> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.inner.encode(encoder)
    }
}

impl<const N: usize, Context> bincode::Decode<Context> for FixedString<N> {
    fn decode<D: bincode::de::Decoder>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let s = String::decode(decoder)?;
        // Truncate if too long instead of returning an error
        Ok(Self::truncate(s))
    }
}

impl<'de, const N: usize, C> bincode::BorrowDecode<'de, C> for FixedString<N> {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let s = String::borrow_decode(decoder)?;
        Ok(Self::truncate(s))
    }
}

pub const DEVICE_NAME_MAX_LENGTH: usize = 14;
pub const KEY_NAME_MAX_LENGTH: usize = 15;

pub type DeviceName = FixedString<DEVICE_NAME_MAX_LENGTH>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_string_creation() {
        let s = FixedString::<10>::from_str("hello").unwrap();
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);

        let err = FixedString::<3>::from_str("hello").unwrap_err();
        assert_eq!(err.max_len, 3);
        assert_eq!(err.actual_len, 5);
    }

    #[test]
    fn test_truncate() {
        let s = FixedString::<3>::truncate("hello".to_string());
        assert_eq!(s.as_str(), "hel");
    }

    #[test]
    fn test_bincode_roundtrip() {
        let s = FixedString::<10>::from_str("test").unwrap();
        let encoded = bincode::encode_to_vec(&s, bincode::config::standard()).unwrap();
        let decoded: FixedString<10> =
            bincode::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded, s);
    }

    #[test]
    fn test_bincode_decode_too_long() {
        let s = "hello world this is too long";
        let encoded = bincode::encode_to_vec(s, bincode::config::standard()).unwrap();
        let result: Result<FixedString<10>, _> =
            bincode::decode_from_slice(&encoded, bincode::config::standard()).map(|x| x.0);
        assert!(result.is_err());
    }
}
