use crate::{
    runtime::ExecSideEffects,
    types::{Hash, Hashing},
};
use core::fmt::{Debug, Display};
use frame_support::StateVersion;
use phala_trie_storage::{deserialize_trie_backend, serialize_trie_backend};
use serde::{Deserialize, Serialize};
use sp_state_machine::{
    backend::Consolidate, Backend as StorageBackend, Ext, OverlayedChanges, StorageTransactionCache,
};

pub type InMemoryBackend = sp_state_machine::InMemoryBackend<Hashing>;

pub trait CommitTransaction: StorageBackend<Hashing> {
    fn commit_transaction(&mut self, root: Hash, transaction: Self::Transaction);
}

impl CommitTransaction for InMemoryBackend {
    fn commit_transaction(&mut self, root: Hash, transaction: Self::Transaction) {
        self.apply_transaction(root, transaction);
    }
}

#[derive(Debug)]
pub struct FallbackableStorageBackend<'be, 'fbe, B, FB> {
    backend: &'be mut B,
    fallback: &'fbe FB,
}

impl<'be, 'fbe, B, FB, Error, Transaction, Hashing> StorageBackend<Hashing>
    for FallbackableStorageBackend<'be, 'fbe, B, FB>
where
    Hashing: hash_db::Hasher,
    B: StorageBackend<Hashing, Error = Error, Transaction = Transaction>,
    FB: StorageBackend<Hashing, Error = Error, Transaction = Transaction>,
    Error: Debug + Display + Send + Sync + 'static,
    Transaction: Consolidate + Default + Send,
{
    type Error = Error;

    type Transaction = Transaction;

    type TrieBackendStorage = B::TrieBackendStorage;

    fn storage(&self, key: &[u8]) -> Result<Option<sp_state_machine::StorageValue>, Self::Error> {
        let fallback = self.fallback;
        self.backend.storage(key).and_then(|v| match v {
            Some(v) => Ok(Some(v)),
            None => fallback.storage(key),
        })
    }

    fn child_storage(
        &self,
        child_info: &sp_core::storage::ChildInfo,
        key: &[u8],
    ) -> Result<Option<sp_state_machine::StorageValue>, Self::Error> {
        self.backend.child_storage(child_info, key)
    }

    fn next_storage_key(
        &self,
        key: &[u8],
    ) -> Result<Option<sp_state_machine::StorageKey>, Self::Error> {
        self.backend.next_storage_key(key)
    }

    fn next_child_storage_key(
        &self,
        child_info: &sp_core::storage::ChildInfo,
        key: &[u8],
    ) -> Result<Option<sp_state_machine::StorageKey>, Self::Error> {
        self.backend.next_child_storage_key(child_info, key)
    }

    fn apply_to_key_values_while<F: FnMut(Vec<u8>, Vec<u8>) -> bool>(
        &self,
        child_info: Option<&sp_core::storage::ChildInfo>,
        prefix: Option<&[u8]>,
        start_at: Option<&[u8]>,
        f: F,
        allow_missing: bool,
    ) -> Result<bool, Self::Error> {
        self.backend
            .apply_to_key_values_while(child_info, prefix, start_at, f, allow_missing)
    }

    fn apply_to_keys_while<F: FnMut(&[u8]) -> bool>(
        &self,
        child_info: Option<&sp_core::storage::ChildInfo>,
        prefix: Option<&[u8]>,
        f: F,
    ) {
        self.backend.apply_to_keys_while(child_info, prefix, f)
    }

    fn for_key_values_with_prefix<F: FnMut(&[u8], &[u8])>(&self, prefix: &[u8], f: F) {
        self.backend.for_key_values_with_prefix(prefix, f)
    }

    fn for_child_keys_with_prefix<F: FnMut(&[u8])>(
        &self,
        child_info: &sp_core::storage::ChildInfo,
        prefix: &[u8],
        f: F,
    ) {
        self.backend
            .for_child_keys_with_prefix(child_info, prefix, f);
    }

    fn storage_root<'a>(
        &self,
        delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
        state_version: StateVersion,
    ) -> (Hashing::Out, Self::Transaction)
    where
        Hashing::Out: Ord,
    {
        self.backend.storage_root(delta, state_version)
    }

    fn child_storage_root<'a>(
        &self,
        child_info: &sp_core::storage::ChildInfo,
        delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
        state_version: StateVersion,
    ) -> (Hashing::Out, bool, Self::Transaction)
    where
        Hashing::Out: Ord,
    {
        self.backend
            .child_storage_root(child_info, delta, state_version)
    }

    fn pairs(&self) -> Vec<(sp_state_machine::StorageKey, sp_state_machine::StorageValue)> {
        self.backend.pairs()
    }

    fn register_overlay_stats(&self, stats: &sp_state_machine::StateMachineStats) {
        self.backend.register_overlay_stats(stats)
    }

    fn usage_info(&self) -> sp_state_machine::UsageInfo {
        self.backend.usage_info()
    }

    fn as_trie_backend(
        &self,
    ) -> Option<&sp_state_machine::TrieBackend<Self::TrieBackendStorage, Hashing>> {
        self.backend.as_trie_backend()
    }
}

impl<'be, 'fbe, B, FB, Error, Transaction> CommitTransaction
    for FallbackableStorageBackend<'be, 'fbe, B, FB>
where
    B: StorageBackend<Hashing, Error = Error, Transaction = Transaction>,
    FB: StorageBackend<Hashing, Error = Error, Transaction = Transaction>,
    Error: Debug + Display + Send + Sync + 'static,
    Transaction: Consolidate + Default + Send,
    B: CommitTransaction,
{
    fn commit_transaction(&mut self, root: Hash, transaction: Self::Transaction) {
        self.backend.commit_transaction(root, transaction)
    }
}

#[derive(Default)]
pub struct Storage<Backend> {
    backend: Backend,
    overlay: OverlayedChanges,
}

impl<Backend> Storage<Backend>
where
    Backend: StorageBackend<Hashing> + CommitTransaction,
{
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            overlay: Default::default(),
        }
    }

    pub fn execute_with<R>(
        &mut self,
        rollback: bool,
        f: impl FnOnce() -> R,
    ) -> (R, ExecSideEffects) {
        let backend = self.backend.as_trie_backend().expect("No trie backend?");

        self.overlay.start_transaction();
        let mut cache = StorageTransactionCache::default();
        let mut ext = Ext::new(&mut self.overlay, &mut cache, backend, None);
        let r = sp_externalities::set_and_run_with_externalities(&mut ext, move || {
            crate::runtime::System::reset_events();
            let r = f();
            (r, crate::runtime::get_side_effects())
        });
        if rollback {
            self.overlay.rollback_transaction()
        } else {
            self.overlay.commit_transaction()
        }
        .expect("BUG: mis-paired transaction");
        r
    }

    pub fn changes_transaction(&self) -> (Hash, Backend::Transaction) {
        let delta = self
            .overlay
            .changes()
            .map(|(k, v)| (&k[..], v.value().map(|v| &v[..])));
        let child_delta = self.overlay.children().map(|(changes, info)| {
            (
                info,
                changes.map(|(k, v)| (&k[..], v.value().map(|v| &v[..]))),
            )
        });

        self.backend
            .full_storage_root(delta, child_delta, sp_core::storage::StateVersion::V0)
    }

    pub fn commit_transaction(&mut self, root: Hash, transaction: Backend::Transaction) {
        self.backend.commit_transaction(root, transaction)
    }

    pub fn clear_changes(&mut self) {
        self.overlay = Default::default();
    }

    pub fn commit_changes(&mut self) {
        let (root, transaction) = self.changes_transaction();
        self.commit_transaction(root, transaction);
        self.clear_changes();
    }

    pub fn set_group_id(&mut self, group_id: &[u8]) {
        self.execute_with(false, || {
            crate::runtime::Pink::set_group_id(group_id);
        });
    }

    pub fn group_id(&mut self) -> Vec<u8> {
        self.execute_with(true, || crate::runtime::Pink::group_id())
            .0
    }
}

impl Serialize for Storage<InMemoryBackend> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let trie = self.backend.as_trie_backend().unwrap();
        serialize_trie_backend(trie, serializer)
    }
}

impl<'de> Deserialize<'de> for Storage<InMemoryBackend> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::new(deserialize_trie_backend(deserializer)?))
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_fallbackable_backend() {
        use super::*;

        let fallback = {
            let backend = InMemoryBackend::default();
            let mut storage = Storage::new(backend);

            storage.set_group_id(&*b"foo");
            storage.commit_changes();

            assert_eq!(storage.group_id(), b"foo".to_vec());

            storage.backend
        };

        let mut backend = {
            let backend = InMemoryBackend::default();
            let mut storage = Storage::new(backend);
            assert_eq!(storage.group_id(), b"".to_vec());
            storage.backend
        };

        let mut storage = Storage::new(FallbackableStorageBackend {
            backend: &mut backend,
            fallback: &fallback,
        });
        // It doesn't work.
        assert_eq!(storage.group_id(), b"foo".to_vec());
    }
}
