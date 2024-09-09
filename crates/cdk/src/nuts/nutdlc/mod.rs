use std::{collections::HashMap, str::FromStr};

use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::hashes::Hash;

use super::nut00::Witness;
use super::{nut00::token::TokenV3Token, nut01::PublicKey, Proof, Proofs};
use super::{nut10, CurrencyUnit, Nut10Secret, SecretData};
use crate::util::hex;
use crate::Amount;
use bitcoin::key::XOnlyPublicKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

pub mod serde_dlc_witness;

#[derive(Debug, Error)]
pub enum Error {}

/// DLC Witness
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DLCWitness {
    /// DLC Secret
    pub dlc_secret: SecretData,
}

impl Proof {
    pub fn add_dlc_witness(&mut self, dlc_secret: Nut10Secret) {
        let secret_data = match dlc_secret.kind {
            nut10::Kind::DLC => (dlc_secret.secret_data),
            _ => todo!("this should error"),
        };
        self.witness = Some(Witness::DLCWitness(DLCWitness {
            dlc_secret: secret_data,
        }));
    }
}

// Ti == SHA256(Ki_ || Pi)
pub struct DLCLeaf {
    pub blinded_locking_point: PublicKey, // TODO: is this the right type to use?
    pub payout: PayoutStructure,          // JSON-encoded payout structure
}

impl DLCLeaf {
    pub fn hash(&self) -> [u8; 32] {
        // Convert blinded_locking_point to bytes
        let point_bytes = self.blinded_locking_point.to_bytes().to_vec();

        // Concatenate point_bytes and payout string
        let mut input = point_bytes;
        input.extend(self.payout.as_bytes());

        // Compute SHA256 hash
        Sha256Hash::hash(&input).to_byte_array()
    }
}

// Tt = SHA256(hash_to_curve(t.to_bytes(8, 'big')) || Pt)
pub struct DLCTimeoutLeaf {
    timeout_hash: PublicKey,
    payout: PayoutStructure,
}

impl DLCTimeoutLeaf {
    pub fn new(timeout: &u64, payout: &PayoutStructure) -> Self {
        let timeout_hash = crate::dhke::hash_to_curve(&timeout.to_be_bytes())
            .expect("error calculating timeout hash");

        Self {
            timeout_hash,
            payout: payout.clone(),
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let mut input = self.timeout_hash.to_bytes().to_vec();
        input.extend(self.payout.as_bytes());
        Sha256Hash::hash(&input).to_byte_array()
    }
}

/// Hash of all spending conditions and blinded locking points
#[derive(Serialize, Deserialize, Debug)]
pub struct DLCRoot([u8; 32]);

impl DLCRoot {
    /// new [`DLCRoot`] from [`DLCLeaf`]s and optional [`DLCTimeoutLeaf`]
    pub fn compute(leaves: Vec<DLCLeaf>, timeout_leaf: Option<DLCTimeoutLeaf>) -> Self {
        let mut input: Vec<[u8; 32]> = Vec::new();
        for leaf in leaves {
            input.push(leaf.hash());
        }
        if let Some(timeout_leaf) = timeout_leaf {
            input.push(timeout_leaf.hash());
        }
        Self {
            0: crate::nuts::nutsct::merkle_root(&input),
        }
    }
}

impl ToString for DLCRoot {
    fn to_string(&self) -> String {
        hex::encode(self.0)
    }
}

impl FromStr for DLCRoot {
    type Err = crate::nuts::nut11::Error;

    fn from_str(s: &str) -> Result<Self, crate::nuts::nut11::Error> {
        let bytes = hex::decode(s).map_err(|_| crate::nuts::nut11::Error::InvalidHash)?;
        if bytes.len() != 32 {
            return Err(crate::nuts::nut11::Error::InvalidHash);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(DLCRoot(array))
    }
}

struct DLCMerkleTree {
    root: DLCRoot,
    leaves: Vec<DLCLeaf>,
    timeout_leaf: Option<DLCTimeoutLeaf>,
}

// NOTE: copied from nut00/token.rs TokenV3, should it be V3 or V4?
pub struct DLCFundingToken {
    /// Proofs in [`Token`] by mint
    pub token: Vec<TokenV3Token>,
    /// Memo for token
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Token Unit
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<CurrencyUnit>,
    /// DLC Root
    pub dlc_root: DLCRoot,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DLC {
    /// DLC Root
    pub dlc_root: String,

    pub funding_amount: Amount,

    pub unit: CurrencyUnit,

    pub inputs: Proofs, // locked with DLC secret - only spendable in this DLC
}

/// see https://github.com/cashubtc/nuts/blob/a86a4e8ce0b9a76ce9b242d6c2c2ab846b3e1955/dlc.md#mint-registration
#[derive(Serialize, Deserialize, Debug)]
pub struct PostDLCRegistrationRequest {
    pub registrations: Vec<DLC>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DLCRegistrationResponse {
    pub funded: Vec<FundedDLC>,
    pub errors: Option<Vec<DLCError>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FundedDLC {
    pub dlc_root: DLCRoot,
    pub funding_proof: FundingProof,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FundingProof {
    pub keyset: String,    // keyset_id_str
    pub signature: String, // bip340_sig_hex
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DLCError {
    pub dlc_root: DLCRoot,
    pub bad_inputs: Vec<BadInput>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BadInput {
    pub index: u32,
    pub detail: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PayoutStructure(HashMap<XOnlyPublicKey, u64>);

impl PayoutStructure {
    /// Create new [`PayoutStructure`] with a single payout
    pub fn default(pubkey: String) -> Self {
        let pubkey = XOnlyPublicKey::from_str(&pubkey).unwrap();
        Self(HashMap::from([(pubkey, 1)]))
    }

    pub fn default_timeout(pubkeys: Vec<String>) -> Self {
        let mut payout = HashMap::new();
        for pubkey in pubkeys {
            let pubkey = XOnlyPublicKey::from_str(&pubkey).unwrap();
            payout.insert(pubkey, 1);
        }
        Self(payout)
    }

    /// Convert the PayoutStructure to a byte representation
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (pubkey, amount) in self.0.iter() {
            bytes.extend_from_slice(&pubkey.serialize());
            bytes.extend_from_slice(&amount.to_le_bytes());
        }
        bytes
    }
}

// Known Parameters
/*
- The number of possible outcomes `n`

- An outcome blinding secret scalar `b`

- A vector of `n` outcome locking points `[K1, K2, ... Kn]`

- A vector of `n` payout structures `[P1, P2, ... Pn]`

- A vector of `n` payout structures `[P1, P2, ... Pn]`

- An optional timeout timestamp `t` and timeout payout structure `Pt`
*/

// b = random secret scalar
// SecretKey::generate()

// blinding points
/*
Ki_ = Ki + b*G
*/
