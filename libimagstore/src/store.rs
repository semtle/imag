use std::collections::HashMap;
use std::fs::{File, remove_file};
use std::ops::Drop;
use std::path::PathBuf;
use std::result::Result as RResult;
use std::sync::Arc;
use std::sync::RwLock;

use fs2::FileExt;

use entry::Entry;
use error::{StoreError, StoreErrorKind};
use storeid::StoreId;
use lazyfile::LazyFile;

/// The Result Type returned by any interaction with the store that could fail
pub type Result<T> = RResult<T, StoreError>;


/// A store entry, depending on the option type it is either borrowed currently
/// or not.
enum StoreEntry {
    Present(StoreId, LazyFile),
    Borrowed
}

impl PartialEq for StoreEntry {
    fn eq(&self, other: &StoreEntry) -> bool {
        use store::StoreEntry::*;
        match (*self, *other) {
            (Borrowed, Borrowed) => true,
            (Borrowed, Present(_,_)) => false,
            (Present(_,_), Borrowed) => false,
            (Present(ref a,_), Present(ref b, _)) => a == b
        }
    }
}


impl StoreEntry {
    /// The entry is currently borrowed, meaning that some thread is currently
    /// mutating it
    fn is_borrowed(&self) -> bool {
        *self == StoreEntry::Borrowed
    }

    fn get_entry(&self) -> Result<Entry> {
        if let &StoreEntry::Present(ref id, ref file) = self {
            let file = file.get_file();
            if let Err(StoreError{err_type: StoreErrorKind::FileNotFound, ..}) = file {
                Ok(Entry::new(id.clone()))
            } else {
                unimplemented!()
            }
        } else {
            return Err(StoreError::new(StoreErrorKind::EntryAlreadyBorrowed, None))
        }
    }
}

/// The Store itself, through this object one can interact with IMAG's entries
pub struct Store {
    location: PathBuf,

    /**
     * Internal Path->File cache map
     *
     * Caches the files, so they remain flock()ed
     *
     * Could be optimized for a threadsafe HashMap
     */
    entries: Arc<RwLock<HashMap<StoreId, StoreEntry>>>,
}

impl Store {

    /// Create a new Store object
    pub fn new(location: PathBuf) -> Result<Store> {
        use std::fs::create_dir_all;

        if !location.exists() {
            let c = create_dir_all(location.clone());
            if c.is_err() {
                return Err(StoreError::new(StoreErrorKind::StorePathCreate,
                                           Some(Box::new(c.err().unwrap()))));
            }
        } else {
            if location.is_file() {
                return Err(StoreError::new(StoreErrorKind::StorePathExists, None));
            }
        }

        Ok(Store {
            location: location,
            entries: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Creates the Entry at the given location (inside the entry)
    pub fn create(&self, entry: Entry) -> Result<()> {
        unimplemented!();
    }

    /// Borrow a given Entry. When the `FileLockEntry` is either `update`d or
    /// dropped, the new Entry is written to disk
    pub fn retrieve<'a>(&'a self, id: StoreId) -> Result<FileLockEntry<'a>> {
        let hsmap = self.entries.write();
        if hsmap.is_err() {
            return Err(StoreError::new(StoreErrorKind::LockPoisoned, None))
        }
        hsmap.unwrap().get_mut(&id)
            .ok_or(StoreError::new(StoreErrorKind::IdNotFound, None))
            .and_then(|store_entry| store_entry.get_entry())
            .and_then(|entry| Ok(FileLockEntry::new(self, entry, id)))
    }

    /// Return the `FileLockEntry` and write to disk
    pub fn update<'a>(&'a self, entry: FileLockEntry<'a>) -> Result<()> {
        self._update(&entry)
    }

    /// Internal method to write to the filesystem store.
    ///
    /// # Assumptions
    /// This method assumes that entry is dropped _right after_ the call, hence
    /// it is not public.
    fn _update<'a>(&'a self, entry: &FileLockEntry<'a>) -> Result<()> {
        unimplemented!();
    }

    /// Retrieve a copy of a given entry, this cannot be used to mutate
    /// the one on disk
    pub fn retrieve_copy(&self, id: StoreId) -> Result<Entry> {
        unimplemented!();
    }

    /// Delete an entry
    pub fn delete(&self, id: StoreId) -> Result<()> {
        let mut entries_lock = self.entries.write();
        let mut entries = entries_lock.unwrap();

        // if the entry is currently modified by the user, we cannot drop it
        if entries.get(&id).map(|e| e.is_borrowed()).unwrap_or(false) {
            return Err(StoreError::new(StoreErrorKind::IdLocked, None));
        }

        // remove the entry first, then the file
        entries.remove(&id);
        remove_file(&id).map_err(|e| StoreError::new(StoreErrorKind::FileError, Some(Box::new(e))))
    }
}

impl Drop for Store {

    /**
     * Unlock all files on drop
     *
     * TODO: Unlock them
     */
    fn drop(&mut self) {
    }

}

/// A struct that allows you to borrow an Entry
pub struct FileLockEntry<'a> {
    store: &'a Store,
    entry: Entry,
    key: StoreId,
}

impl<'a> FileLockEntry<'a, > {
    fn new(store: &'a Store, entry: Entry, key: StoreId) -> FileLockEntry<'a> {
        FileLockEntry {
            store: store,
            entry: entry,
            key: key,
        }
    }
}

impl<'a> ::std::ops::Deref for FileLockEntry<'a> {
    type Target = Entry;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl<'a> ::std::ops::DerefMut for FileLockEntry<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry
    }
}

impl<'a> Drop for FileLockEntry<'a> {
    fn drop(&mut self) {
        self.store._update(self).unwrap()
    }
}
