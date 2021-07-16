use crate::light_validation::LightValidation;
use enclave_api::storage_sync::{BlockValidator, Error as SyncError, Result};
use parity_scale_codec::Decode;

impl BlockValidator for LightValidation<chain::Runtime> {
    fn submit_finalized_headers(
        &mut self,
        bridge_id: u64,
        header: chain::Header,
        ancestry_proof: sp_application_crypto::Vec<chain::Header>,
        grandpa_proof: sp_application_crypto::Vec<u8>,
        auhtority_set_change: Option<enclave_api::blocks::AuthoritySetChange>,
    ) -> Result<()> {
        self.submit_finalized_headers(
            bridge_id,
            header,
            ancestry_proof,
            grandpa_proof,
            auhtority_set_change,
        )
        .map_err(|_| SyncError::HeaderValidateFailed)
    }

    fn validate_storage_proof(
        &self,
        state_root: chain::Hash,
        mut proof: &[u8],
        items: &[(&[u8], &[u8])],
    ) -> Result<()> {
        let proof = Decode::decode(&mut proof).map_err(|_| SyncError::CodecError)?;
        self.validate_storage_proof(state_root, proof, items)
            .map_err(|_| SyncError::HeaderValidateFailed)
    }
}