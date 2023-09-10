use std::{ops::Deref, sync::Arc};

use cashu_sdk::types::ProofsStatus as ProofsStatusSdk;

use crate::MintProof;

pub struct ProofsStatus {
    inner: ProofsStatusSdk,
}

impl ProofsStatus {
    pub fn new(spendable: Vec<Arc<MintProof>>, spent: Vec<Arc<MintProof>>) -> Self {
        Self {
            inner: ProofsStatusSdk {
                spendable: spendable
                    .iter()
                    .map(|p| p.as_ref().deref().clone())
                    .collect(),
                spent: spent.iter().map(|p| p.as_ref().deref().clone()).collect(),
            },
        }
    }

    pub fn spendable(&self) -> Vec<Arc<MintProof>> {
        self.inner
            .spendable
            .clone()
            .into_iter()
            .map(|p| Arc::new(p.into()))
            .collect()
    }

    pub fn spent(&self) -> Vec<Arc<MintProof>> {
        self.inner
            .spent
            .clone()
            .into_iter()
            .map(|p| Arc::new(p.into()))
            .collect()
    }
}