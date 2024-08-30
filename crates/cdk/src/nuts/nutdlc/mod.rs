use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::hashes::Hash;

use super::{nut00::token::TokenV3Token, nut01::PublicKey, nutsct::merkle_root, Proofs};
use crate::Amount;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::CurrencyUnit;

#[derive(Debug, Error)]
pub enum Error {}

/// DLC Witness
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DLCWitness {
    /// Signatures
    signatures: Vec<String>,
}

// Ti == SHA256(Ki_ || Pi)
struct DLCLeaf {
    blinded_locking_point: PublicKey, // TODO: is this the right type to use?
    payout: String,                   // JSON-encoded payout structure
}

impl DLCLeaf {
    fn hash(&self) -> String {
        // Convert blinded_locking_point to bytes
        let point_bytes = self.blinded_locking_point.to_bytes().to_vec();

        // Concatenate point_bytes and payout string
        let mut input = point_bytes;
        input.extend_from_slice(self.payout.as_bytes());

        // Compute SHA256 hash
        let hash: [u8; 32] = Sha256Hash::hash(&input).to_byte_array();

        // Convert to hexadecimal string
        crate::util::hex::encode(hash)
    }
}

// Tt = SHA256(hash_to_curve(t.to_bytes(8, 'big')) || Pt)
struct DLCTimeoutLeaf {
    timeout: u64, // Unix timestamp
    // TODO: is there a JSON type I should use?
    payout: String, // JSON-encoded timeout payout structure
}

struct DLCRoot(String);

impl DLCRoot {
    fn compute(leaves: Vec<DLCLeaf>, timeout_leaf: Option<DLCTimeoutLeaf>) -> Self {
        todo!()
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

// struct DLCSpendingConditions {
//     data: DLCRoot,
//     conditions: Option<SpendingConditions>,
// }

struct DLC {
    /// DLC Root
    pub dlc_root: DLCRoot,

    funding_amount: Amount,

    unit: CurrencyUnit,

    inputs: Proofs, // locked with DLC secret - only spendable in this DLC
}

/// see https://github.com/cashubtc/nuts/blob/a86a4e8ce0b9a76ce9b242d6c2c2ab846b3e1955/dlc.md#mint-registration
struct PostDlcRegistrationRequest {
    registrations: Vec<DLC>,
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
