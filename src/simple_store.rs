use async_trait::async_trait;
use parking_lot::{RwLock, lock_api::RwLockReadGuard};
use std::marker::{Send, Sync};
use std::time::Duration;
use std::{path::PathBuf, sync::Arc};
use tokio::time::sleep;

/// Exposes a thread-safe store that loads itself on initalization
/// (if it exists) and can be refreshed on demand. When refreshed
/// a working copy is stored on disk while the memory representation
/// is updated.
pub struct Store<T>(Arc<RwLock<T>>, PathBuf);

impl<T> Clone for Store<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}

impl<T: Default + TryFrom<Vec<u8>, Error = anyhow::Error>> Store<T>
where
    for<'a> Vec<u8>: From<&'a T>,
{
    /// To run the initial loading the store, or running and update if needed
    /// By the end of this routine the store will be loaded and data stashed to disk,
    /// otherwise an error is raised.
    pub fn new_with_default(loc: PathBuf) -> Result<Store<T>, anyhow::Error> {
        Store::new_or_get(loc, || Ok(T::default()))
    }
}

impl<T: TryFrom<Vec<u8>, Error = anyhow::Error>> Store<T>
where
    for<'a> Vec<u8>: From<&'a T>,
{
    pub fn new_or_get<F>(loc: PathBuf, getter: F) -> Result<Store<T>, anyhow::Error>
    where
        F: FnOnce() -> Result<T, anyhow::Error>,
    {
        let data = match std::fs::read(&loc) {
            Err(_) => {
                // Assume store missing, let's run an update
                let new_data = getter()?;
                let serialized: Vec<u8> = (&new_data).into();
                std::fs::write(&loc, serialized)?;
                new_data
            }
            Ok(v) => T::try_from(v)?,
        };
        Ok(Store(Arc::new(RwLock::new(data)), loc))
    }
}

impl<T: TryFrom<Vec<u8>, Error = anyhow::Error>> Store<T>
where
    for<'a> Vec<u8>: From<&'a T>,
{
    pub async fn new_with_fetcher<F>(loc: PathBuf, fetcher: F) -> Result<Store<T>, anyhow::Error>
    where
        F: Fetcher<T>,
    {
        let data = match std::fs::read(&loc) {
            Err(_) => {
                // Assume store missing, let's run an update
                let new_data = fetcher.fetch(None).await?;
                let serialized: Vec<u8> = (&new_data).into();
                std::fs::write(&loc, serialized)?;
                new_data
            }
            Ok(v) => T::try_from(v)?,
        };
        Ok(Store(Arc::new(RwLock::new(data)), loc))
    }
}

impl<T> Store<T>
where
    for<'a> Vec<u8>: From<&'a T>,
{
    pub fn write(&self, new_data: T) -> Result<(), anyhow::Error> {
        let serialized: Vec<u8> = (&new_data).into();
        std::fs::write(&self.1, serialized)?;
        {
            let mut w = self.0.write();
            *w = new_data;
        }
        Ok(())
    }
}

impl<T> Store<T> {
    pub fn read(&self) -> RwLockReadGuard<'_, parking_lot::RawRwLock, T> {
        self.0.read()
    }
}

#[async_trait]
pub trait Fetcher<T> {
    async fn fetch(&self, store: Option<Store<T>>) -> Result<T, anyhow::Error>;
}
impl<T: Send + Sync + 'static> Store<T> {
    pub fn scheduled_updates<F>(&self, fetcher: F, between: Duration)
    where
        F: Fetcher<T> + Send + Sync + 'static + Clone,
        for<'a> Vec<u8>: From<&'a T>,
    {
        let mvfetch = fetcher.clone();
        let mvstore = self.clone();
        tokio::spawn(async move {
            loop {
                sleep(between).await;
                if let Err(_e) = mvfetch
                    .fetch(Some(mvstore.clone()))
                    .await
                    .and_then(|v| mvstore.write(v))
                {
                    todo!()
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MyData {
        data: Vec<String>,
        len: usize,
    }
    impl TryFrom<Vec<u8>> for MyData {
        type Error = anyhow::Error;

        fn try_from(_value: Vec<u8>) -> Result<Self, Self::Error> {
            // serde_json::from_slice(&value) --- for example
            // Likely bincode options, etc
            todo!()
        }
    }

    impl<'a> From<&'a MyData> for Vec<u8> {
        fn from(_value: &'a MyData) -> Self {
            // serde_json::to_vec(&value) --- for example
            todo!()
        }
    }

    #[derive(Clone)]
    struct DataFetcher; // Hold an HTTP client like reqwest; cheap to clone

    #[async_trait]
    impl Fetcher<MyData> for DataFetcher {
        async fn fetch(&self, _store: Option<Store<MyData>>) -> Result<MyData, anyhow::Error> {
            todo!()
        }
    }

    fn sync_store() -> Result<(), anyhow::Error> {
        let s: Store<MyData> = Store::new_with_default(PathBuf::new())?;
        s.write(MyData::default())?;
        let dat = &s.read().data;
        Ok(())
    }

    async fn updating_store() -> Result<(), anyhow::Error> {
        let f = DataFetcher;
        let s: Store<MyData> = Store::new_with_fetcher(PathBuf::new(), f.clone()).await?;
        s.scheduled_updates(f, Duration::from_secs(180));
        s.read(); // Grab a read lock
        Ok(())
    }
}
