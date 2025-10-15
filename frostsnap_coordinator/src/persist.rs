use std::{io::BufReader, ops::Deref, str::FromStr};

use anyhow::Result;
use bdk_chain::bitcoin::{
    consensus::{Decodable, Encodable},
    Txid,
};
use rusqlite::{
    types::{FromSql, FromSqlError, ToSqlOutput},
    ToSql,
};

pub struct Persisted<T>(T);

impl<T> Persisted<T> {
    pub fn mutate<C, R, U>(
        &mut self,
        db: &mut C,
        mutator: impl FnOnce(&mut T) -> Result<(R, U)>,
    ) -> Result<R>
    where
        T: Persist<C>,
        U: Into<T::Update>,
    {
        let (ret, update) = (mutator)(&mut self.0)?;
        T::persist_update(db, update.into())?;
        Ok(ret)
    }

    pub fn mutate2<C, R>(
        &mut self,
        db: &mut C,
        mutator: impl FnOnce(&mut T, &mut T::Update) -> Result<R>,
    ) -> Result<R>
    where
        T: Persist<C>,
        T::Update: Default,
    {
        let mut update = T::Update::default();
        let result = (mutator)(&mut self.0, &mut update);
        T::persist_update(db, update)?;
        result
    }

    pub fn staged_mutate<C, R>(
        &mut self,
        db: &mut C,
        mutator: impl FnOnce(&mut T) -> Result<R>,
    ) -> Result<R>
    where
        T: Persist<C> + TakeStaged<T::Update>,
    {
        let ret = mutator(&mut self.0)?;
        let update = self.0.take_staged_update();
        if let Some(update) = update {
            T::persist_update(db, update)?;
        }
        Ok(ret)
    }

    #[allow(non_snake_case)]
    /// Scary upppercase method that allows you opt-out of persisting anything at the end of a mutation
    pub fn MUTATE_NO_PERSIST(&mut self) -> &mut T {
        &mut self.0
    }

    pub fn multi<'a, 'b, B>(
        &'a mut self,
        other: &'b mut Persisted<B>,
    ) -> Multi<(&'a mut Self, &'b mut Persisted<B>)> {
        Multi((self, other))
    }

    pub fn new<C>(db: &mut C, params: T::LoadParams) -> Result<Self>
    where
        T: Persist<C>,
    {
        T::migrate(db)?;
        Ok(Persisted(T::load(db, params)?))
    }
}

pub struct Multi<L>(L);

macro_rules! impl_multi {
    ($($name:tt $uname:ident $index:tt),+) => {
        #[allow(unused_parens)]
        impl<'a, $($name),+> Multi<($(&'a mut Persisted<$name>,)+)> {
            #[allow(non_snake_case)]
            pub fn mutate<Conn, R, $($uname),+>(
                &mut self,
                db: &mut Conn,
                mutator: impl FnOnce($(&mut $name),+) -> Result<(R, ($($uname),+))>,
            ) -> Result<R>
            where
                $(
                    $uname: Into<$name::Update>,
                    $name: Persist<Conn>,
                )+
            {
                #[allow(non_snake_case)]
                let (ret, ($($uname),+)) = mutator($(&mut self.0.$index.0),+)?;
                $(
                    $name::persist_update(db, $uname.into())?;
                )+
                Ok(ret)
            }

            pub fn multi<'n, N>(self, next: &'n mut Persisted<N>) -> Multi<($(&'a mut Persisted<$name>,)+ &'n mut Persisted<N>)> {
                Multi(($(self.0.$index,)+ next))
            }
        }
    };
}

// Generate the implementations for tuples up to 10 items, including single element tuple
impl_multi!(A UA 0);
impl_multi!(A UA 0, B UB 1);
impl_multi!(A UA 0, B UB 1, C UC 2);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3, E UE 4);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3, E UE 4, F UF 5);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3, E UE 4, F UF 5, G UG 6);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3, E UE 4, F UF 5, G UG 6, H UH 7);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3, E UE 4, F UF 5, G UG 6, H UH 7, I UI 8);
impl_multi!(A UA 0, B UB 1, C UC 2, D UD 3, E UE 4, F UF 5, G UG 6, H UH 7, I UI 8, J UJ 9);

impl<T> AsRef<T> for Persisted<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Persisted<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait Persist<C> {
    type Update;
    type LoadParams;

    fn migrate(conn: &mut C) -> Result<()>;

    fn load(conn: &mut C, params: Self::LoadParams) -> Result<Self>
    where
        Self: Sized;
    fn persist_update(conn: &mut C, update: Self::Update) -> Result<()>;
}

pub trait TakeStaged<U> {
    fn take_staged_update(&mut self) -> Option<U>;
}

pub struct SqlBitcoinTransaction<T = bdk_chain::bitcoin::Transaction>(pub T);

impl FromSql for SqlBitcoinTransaction {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Blob(blob) => {
                let tx =
                    bdk_chain::bitcoin::Transaction::consensus_decode(&mut BufReader::new(blob))
                        .map_err(|e| FromSqlError::Other(Box::new(e)))?;
                Ok(SqlBitcoinTransaction(tx))
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl<T: Deref<Target = bdk_chain::bitcoin::Transaction>> ToSql for SqlBitcoinTransaction<T> {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        let mut buf = Vec::<u8>::new();
        self.0
            .consensus_encode(&mut buf)
            .expect("transaction can be encoded");
        Ok(ToSqlOutput::from(buf))
    }
}

pub struct SqlTxid(pub bdk_chain::bitcoin::Txid);

impl FromSql for SqlTxid {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(SqlTxid(
            Txid::from_str(value.as_str()?).map_err(|e| FromSqlError::Other(Box::new(e)))?,
        ))
    }
}

impl ToSql for SqlTxid {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_string()))
    }
}

pub struct SqlBlockHash(pub bdk_chain::bitcoin::BlockHash);

impl FromSql for SqlBlockHash {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(SqlBlockHash(
            bdk_chain::bitcoin::BlockHash::from_str(value.as_str()?)
                .map_err(|e| FromSqlError::Other(Box::new(e)))?,
        ))
    }
}

impl ToSql for SqlBlockHash {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_string()))
    }
}

pub struct SqlDescriptorId(pub bdk_chain::DescriptorId);

impl FromSql for SqlDescriptorId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(SqlDescriptorId(
            bdk_chain::DescriptorId::from_str(value.as_str()?)
                .map_err(|e| FromSqlError::Other(Box::new(e)))?,
        ))
    }
}

impl ToSql for SqlDescriptorId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_string()))
    }
}

pub struct SqlSignSessionId(pub frostsnap_core::SignSessionId);

impl FromSql for SqlSignSessionId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        use frostsnap_core::SignSessionId;

        let blob_data = value.as_blob()?;
        if blob_data.len() != SignSessionId::LEN {
            return Err(FromSqlError::InvalidBlobSize {
                expected_size: SignSessionId::LEN,
                blob_size: blob_data.len(),
            });
        }

        Ok(SqlSignSessionId(
            SignSessionId::from_slice(blob_data).expect("already checked len"),
        ))
    }
}

impl ToSql for SqlSignSessionId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_bytes().to_vec()))
    }
}

pub struct SqlPsbt(pub bdk_chain::bitcoin::Psbt);

impl FromSql for SqlPsbt {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        use bdk_chain::bitcoin::Psbt;
        let bytes = value.as_bytes()?;
        println!("PSBT size: {}", bytes.len());
        let psbt =
            Psbt::deserialize(value.as_bytes()?).map_err(|e| FromSqlError::Other(Box::new(e)))?;
        Ok(SqlPsbt(psbt))
    }
}

impl ToSql for SqlPsbt {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.serialize()))
    }
}

pub struct BincodeWrapper<T>(pub T);

impl<T: bincode::Encode> ToSql for BincodeWrapper<T> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let bytes = bincode::encode_to_vec(&self.0, bincode::config::standard()).unwrap();
        Ok(ToSqlOutput::from(bytes))
    }
}

impl<T: bincode::Decode<()>> FromSql for BincodeWrapper<T> {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let (decoded, _len) =
            bincode::decode_from_slice::<T, _>(value.as_blob()?, bincode::config::standard())
                .map_err(|e| FromSqlError::Other(Box::new(e)))?;

        Ok(Self(decoded))
    }
}

pub struct ToStringWrapper<T>(pub T);

impl<T: ToString> ToSql for ToStringWrapper<T> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_string()))
    }
}

impl<T: FromStr> FromSql for ToStringWrapper<T>
where
    T::Err: std::error::Error + Send + 'static + Sync,
{
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let decoded = T::from_str(value.as_str()?).map_err(|e| FromSqlError::Other(Box::new(e)))?;
        Ok(Self(decoded))
    }
}
