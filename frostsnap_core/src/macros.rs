/// Implements Display, FromStr, Serialize and Deserialize for something that
/// can be represented as a fixed length byte array
#[macro_export]
#[doc(hidden)]
macro_rules! impl_fromstr_deserialize {
    (
        name => $name:literal,
        fn from_bytes$(<$($tpl:ident  $(: $tcl:ident)?),*>)?($input:ident : [u8;$len:literal]) ->  Option<$type:path> $block:block
    ) => {

        impl$(<$($tpl $(:$tcl)?),*>)?  $type {

            pub fn from_bytes($input: [u8; $len]) -> Option<$type> {
                #[allow(clippy::redundant_closure_call)]
                (|| { $block })()
            }
        }

        $crate::impl_fromstr_deserialize!(@main name => $name, fn from_bytes$(<$($tpl $(:$tcl)?),*>)?($input: [u8;$len]) -> Option<$type> $block);
    };
    (name => $name:literal,
     fn from_bytes$(<$($tpl:ident  $(: $tcl:ident)?),*>)?($input:ident : [u8;$len:literal]) ->  $type:path $block:block
    ) => {
         impl$(<$($tpl $(:$tcl)?),*>)?  $type {

            pub fn from_bytes($input: [u8; $len]) -> $type {
                #[allow(clippy::redundant_closure_call)]
                (|| { $block })()
            }
        }
        $crate::impl_fromstr_deserialize!(@main name => $name, fn from_bytes$(<$($tpl $(:$tcl)?),*>)?($input: [u8;$len]) -> Option<$type> { Some($block) });
    };
    (
        @main
        name => $name:literal,
        fn from_bytes$(<$($tpl:ident  $(: $tcl:ident)?),*>)?($input:ident : [u8;$len:literal]) ->  Option<$type:path> $block:block
    ) => {

        impl$(<$($tpl $(:$tcl)?),*>)?  $type {

            pub fn from_slice(slice: &[u8]) -> Option<$type> {
                if slice.len() != $len {
                    return None;
                }
                let mut $input = [0u8;$len];
                $input.copy_from_slice(&slice[..]);
                #[allow(clippy::redundant_closure_call)]
                (|| { $block })()
            }
        }

        impl$(<$($tpl $(:$tcl)?),*>)? core::str::FromStr for $type  {
            type Err = $crate::hex::HexError;

            /// Parses the string as hex and interprets tries to convert the
            /// resulting byte array into the desired value.
            fn from_str(hex: &str) -> Result<$type , $crate::hex::HexError> {
                use $crate::hex::hex_val;
                if hex.len() % 2 == 1 {
                    Err($crate::hex::HexError::InvalidHex)
                } else if $len * 2 != hex.len() {
                    Err($crate::hex::HexError::InvalidLength)
                } else {
                    let mut buf = [0u8; $len];

                    for (i, hex_byte) in hex.as_bytes().chunks(2).enumerate() {
                        buf[i] = hex_val(hex_byte[0])? << 4 | hex_val(hex_byte[1])?
                    }

                    let $input = buf;
                    #[allow(clippy::redundant_closure_call)]
                    let result = (|| { $block })();
                    result.ok_or($crate::hex::HexError::InvalidEncoding)
                }
            }
        }

        impl<'de, $($($tpl $(: $tcl)?),*)?> $crate::serde::Deserialize<'de> for $type  {
            fn deserialize<Deser: $crate::serde::Deserializer<'de>>(
                deserializer: Deser,
            ) -> Result<$type , Deser::Error> {


                {
                    if deserializer.is_human_readable() {
                        #[allow(unused_parens)]
                        struct HexVisitor$(<$($tpl),*>)?$((core::marker::PhantomData<($($tpl),*)> ))?;
                        impl<'de, $($($tpl $(: $tcl)?),*)?> $crate::serde::de::Visitor<'de> for HexVisitor$(<$($tpl),*>)? {
                            type Value = $type ;
                            fn expecting(
                                &self,
                                f: &mut core::fmt::Formatter,
                            ) -> core::fmt::Result {
                                write!(f, "a valid {}-byte hex encoded {}", $len, $name)?;
                                Ok(())
                            }

                            fn visit_str<E: $crate::serde::de::Error>(self, v: &str) -> Result<$type , E> {
                                use $crate::hex::HexError::*;
                                <$type  as core::str::FromStr>::from_str(v).map_err(|e| match e {
                                    InvalidLength => E::invalid_length(v.len() / 2, &self),
                                    InvalidEncoding => E::invalid_value($crate::serde::de::Unexpected::Str(v), &self),
                                    InvalidHex => E::custom("invalid hex")
                                })
                            }
                        }

                        #[allow(unused_parens)]
                        return deserializer.deserialize_str(HexVisitor$((core::marker::PhantomData::<($($tpl),*)>))?);
                    }
                }

                {
                    #[allow(unused_parens)]
                    struct BytesVisitor$(<$($tpl),*>)?$((core::marker::PhantomData<($($tpl),*)> ))?;

                    impl<'de, $($($tpl $(: $tcl)?),*)?> $crate::serde::de::Visitor<'de> for BytesVisitor$(<$($tpl),*>)? {
                        type Value = $type ;

                        fn expecting(
                            &self,
                            f: &mut core::fmt::Formatter,
                        ) -> core::fmt::Result {
                            write!(f, "a valid {}-byte encoding of a {}", $len, $name)?;
                            Ok(())
                        }

                        fn visit_seq<A>(self, mut seq: A) -> Result<$type , A::Error>
                        where A: $crate::serde::de::SeqAccess<'de> {

                            let mut $input = [0u8; $len];
                            for i in 0..$len {
                                $input[i] = seq.next_element()?
                                .ok_or_else(|| $crate::serde::de::Error::invalid_length(i, &self))?;
                            }

                            #[allow(clippy::redundant_closure_call)]
                            let result = (|| { $block })();
                            result.ok_or($crate::serde::de::Error::custom(format_args!("invalid byte encoding, expected {}", &self as &dyn $crate::serde::de::Expected)))
                        }
                    }

                    #[allow(unused_parens)]
                    deserializer.deserialize_tuple($len, BytesVisitor$((core::marker::PhantomData::<($($tpl),*)>))?)
                }
            }
        }

        impl$(<$($tpl $(:$tcl)?),*>)? $crate::bincode::de::Decode for $type {
            fn decode<D: $crate::bincode::de::Decoder>(decoder: &mut D) -> Result<Self, $crate::bincode::error::DecodeError> {
                use $crate::bincode::de::read::Reader;
                let mut $input = [0u8; $len];
                decoder.reader().read(&mut $input)?;
                #[allow(clippy::redundant_closure_call)]
                let result = (|| { $block })();
                return result.ok_or($crate::bincode::error::DecodeError::OtherString(format!("Invalid {}-byte encoding of a {}", $len, $name)));
            }
        }

        impl<'de, $($($tpl $(:$tcl)?),*)?> $crate::bincode::BorrowDecode<'de> for $type {
            fn borrow_decode<D: $crate::bincode::de::BorrowDecoder<'de>>(
                decoder: &mut D,
            ) -> core::result::Result<Self, $crate::bincode::error::DecodeError> {
                $crate::bincode::Decode::decode(decoder)
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! impl_display_serialize {
    (fn to_bytes$(<$($tpl:ident  $(: $tcl:ident)?),*>)?($self:ident : &$type:path) -> [u8;$len:literal] $block:block) => {

        impl$(<$($tpl $(:$tcl)?),*>)? $type {
            pub fn to_bytes(&self) -> [u8; $len] {
                let $self = self;
                $block
            }
        }

        impl$(<$($tpl $(:$tcl)?),*>)? $crate::serde::Serialize for $type {
            fn serialize<Ser: $crate::serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
                use $crate::serde::ser::SerializeTuple;
                let $self = &self;
                let bytes = $block;

                {
                    use $crate::hex;
                    if serializer.is_human_readable() {
                        return serializer.serialize_str(&hex::encode(&bytes[..]))
                    }
                }

                //NOTE: idea taken from https://github.com/dalek-cryptography/curve25519-dalek/pull/297/files
                let mut tup = serializer.serialize_tuple($len)?;
                for byte in bytes.iter() {
                    tup.serialize_element(byte)?;
                }
                tup.end()
            }
        }

        impl$(<$($tpl $(:$tcl)?),*>)? core::fmt::Display for $type {
            /// Displays as hex.
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                let $self = &self;
                let bytes = $block;
                for byte in bytes.iter() {
                    write!(f, "{:02x}", byte)?
                }
                Ok(())
            }
        }

        impl$(<$($tpl $(:$tcl)?),*>)? $crate::bincode::Encode for $type {
            fn encode<E: $crate::bincode::enc::Encoder>(&self, encoder: &mut E) -> Result<(), $crate::bincode::error::EncodeError> {
                use $crate::bincode::enc::write::Writer;
                let $self = &self;
                let bytes = $block;
                encoder.writer().write(bytes.as_ref())
            }
        }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! impl_debug {
    (fn to_bytes$(<$($tpl:ident  $(: $tcl:ident)?),*>)?($self:ident : &$type_name:ident$(<$($tpr:path),+>)?) -> $($tail:tt)*) => {
        impl$(<$($tpl $(:$tcl)?),*>)? core::fmt::Debug for $type_name$(<$($tpr),+>)? {
            /// Formats the type as hex and any markers on the type.
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                let $self = &self;
                write!(f, "{}", stringify!($type_name))?;
                $(
                    write!(f, "<")?;
                    $crate::impl_debug!(@recursive_print f, $(core::any::type_name::<$tpr>().rsplit("::").next().unwrap()),*);
                    write!(f, ">")?;
                )?
                    write!(f, "(")?;
                $crate::impl_debug!(@output f, $self, $($tail)*);
                write!(f, ")")?;
                Ok(())
            }
        }
    };
    (@output $f:ident, $self:ident, Result<$(&)?[u8;$len:literal], &str> $block:block) => {
        let res: Result<[u8;$len], &str> = $block;
        match res {
            Ok(bytes) => {
                for byte in bytes.iter() {
                    write!($f, "{:02x}", byte)?
                }
            },
            Err(string) => {
                write!($f, "{}", string)?
            }
        }
    };
    (@output $f:ident, $self:ident, $(&)?[u8;$len:literal] $block:block) => {
        let bytes = $block;
        for byte in bytes.iter() {
            write!($f, "{:02x}", byte)?
        }
    };
    (@recursive_print $f:ident, $next:expr, $($tt:tt)+) => {
        $f.write_str($next)?;
        $f.write_str(",")?;
        $crate::impl_debug!(@recursive_print $f, $($tt)+)
    };
    (@recursive_print $f:ident, $next:expr) => {
        $f.write_str($next)?;
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! impl_display_debug_serialize {
    ($($tt:tt)+) => {
        $crate::impl_display_serialize!($($tt)+);
        $crate::impl_debug!($($tt)+);
    };
}
