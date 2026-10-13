//! A JSON-file token store for debug builds only, selected with `SPEQ_DEV_CREDENTIAL_FILE`. Integration tests run the
//! real binary as Git's remote and credential helper: separate processes that cannot share an in-memory store, and
//! must not touch the developer's OS credential store. Release builds do not contain this module.
use crate::credentials::{CredentialStore, StoredCredential};
use crate::error::{Error, Result};
use crate::fsx::{self, Visibility};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const VARIABLE: &str = "SPEQ_DEV_CREDENTIAL_FILE";

pub struct FileStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> FileStore {
        FileStore {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    /// The store named by [`VARIABLE`], if set.
    pub fn from_env() -> Option<Arc<dyn CredentialStore>> {
        let path = std::env::var_os(VARIABLE)?;
        Some(Arc::new(FileStore::new(path)))
    }

    fn read(&self) -> Result<BTreeMap<String, StoredCredential>> {
        match fsx::read_optional(&self.path)? {
            None => Ok(BTreeMap::new()),
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| Error::CredentialStore(format!("{}: {e}", self.path.display()))),
        }
    }

    fn write(&self, all: &BTreeMap<String, StoredCredential>) -> Result<()> {
        let bytes = serde_json::to_vec(all).map_err(|e| Error::CredentialStore(e.to_string()))?;
        fsx::atomic_write(&self.path, &bytes, Visibility::Private)
    }
}

impl CredentialStore for FileStore {
    fn load(&self, account: &str) -> Result<Option<StoredCredential>> {
        let _guard = self.lock.lock().expect("store lock");
        Ok(self.read()?.remove(account))
    }

    fn save(&self, account: &str, credential: &StoredCredential) -> Result<()> {
        let _guard = self.lock.lock().expect("store lock");
        let mut all = self.read()?;
        all.insert(account.to_string(), credential.clone());
        self.write(&all)
    }

    fn delete(&self, account: &str) -> Result<()> {
        let _guard = self.lock.lock().expect("store lock");
        let mut all = self.read()?;
        if all.remove(account).is_some() {
            self.write(&all)?;
        }
        Ok(())
    }
}
