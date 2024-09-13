pub mod serde_sct_witness;

use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::hashes::Hash;
use serde::{Deserialize, Serialize};

use crate::secret::Secret;

use super::{nut10, Nut10Secret, Proof, Token, Witness};

// In its _expanded_ form, a Spending Condition Tree (SCT) is an ordered list of [NUT-00] secrets, `[x1, x2, ... xn]`.
pub struct SpendingConditionTree {
    conditions: Vec<Token>, //Should be ordered
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SCTWitness {
    leaf_secret: String,
    merkle_proof: Vec<String>,
}

impl Proof {
    pub fn add_sct_witness(&mut self, leaf_secret: String, merkle_proof: Vec<String>) {
        self.witness = Some(Witness::SCTWitness(SCTWitness {
            leaf_secret,
            merkle_proof,
        }));
    }
}

pub fn sorted_merkle_hash(left: &[u8], right: &[u8]) -> [u8; 32] {
    // sort the inputs
    let (left, right) = if left < right {
        (left, right)
    } else {
        (right, left)
    };

    // concatenate the inputs
    let mut to_hash = Vec::new();
    to_hash.extend_from_slice(left);
    to_hash.extend_from_slice(right);

    // hash the concatenated inputs
    Sha256Hash::hash(&to_hash).to_byte_array()
}

/// see https://github.com/cashubtc/nuts/blob/a86a4e8ce0b9a76ce9b242d6c2c2ab846b3e1955/sct.md#merkle_rootleaf_hashes-listbytes---bytes
pub fn merkle_root(leaf_hashes: &[[u8; 32]]) -> [u8; 32] {
    if leaf_hashes.is_empty() {
        return [0; 32];
    } else if leaf_hashes.len() == 1 {
        return leaf_hashes[0].to_owned();
    } else {
        let split = leaf_hashes.len() / 2; // TODO: will this round?
        let left = merkle_root(&leaf_hashes[..split]);
        let right = merkle_root(&leaf_hashes[split..]);
        sorted_merkle_hash(&left, &right)
    }
}

// see https://github.com/cashubtc/nuts/blob/a86a4e8ce0b9a76ce9b242d6c2c2ab846b3e1955/sct.md#merkle_verifyroot-bytes-leaf_hash-bytes-proof-listbytes---bool
pub fn merkle_verify(root: &[u8; 32], leaf_hash: &[u8; 32], proof: &Vec<String>) -> bool {
    let mut current_hash = *leaf_hash;
    for branch_hash_hex in proof {
        let branch_hash = crate::util::hex::decode(branch_hash_hex).expect("Invalid hex string");
        current_hash = sorted_merkle_hash(&current_hash, &branch_hash);
    }

    current_hash == *root
}

pub fn merkle_prove(leaf_hashes: Vec<[u8; 32]>, position: usize) -> Vec<[u8; 32]> {
    if leaf_hashes.len() <= 1 {
        return Vec::new();
    }
    let split = leaf_hashes.len() / 2;

    if position < split {
        let mut proof = merkle_prove(leaf_hashes[..split].to_vec(), position);
        proof.push(merkle_root(&leaf_hashes[split..]));
        return proof;
    } else {
        let mut proof = merkle_prove(leaf_hashes[split..].to_vec(), position - split);
        proof.push(merkle_root(&leaf_hashes[..split]));
        return proof;
    }
}

pub fn sct_root(secrets: Vec<Secret>) -> [u8; 32] {
    let leaf_hashes: Vec<[u8; 32]> = secrets
        .iter()
        .map(|s| Sha256Hash::hash(&s.to_bytes()).to_byte_array())
        .collect();

    merkle_root(&leaf_hashes)
}

pub fn sct_leaf_hashes(secrets: Vec<Secret>) -> Vec<[u8; 32]> {
    secrets
        .iter()
        .map(|s| Sha256Hash::hash(&s.as_bytes()).to_byte_array())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{env::consts::EXE_EXTENSION, str::FromStr};

    use lightning_invoice::Sha256;

    use crate::util::hex;

    use super::*;

    //https://github.com/cashubtc/nuts/blob/a86a4e8ce0b9a76ce9b242d6c2c2ab846b3e1955/tests/sct-tests.md.md
    #[test]
    fn test_secret_hash() {
        let s = "[\"P2PK\",{\"nonce\":\"ffd73b9125cc07cdbf2a750222e601200452316bf9a2365a071dd38322a098f0\",\"data\":\"028fab76e686161cc6daf78fea08ba29ce8895e34d20322796f35fec8e689854aa\",\"tags\":[[\"sigflag\",\"SIG_INPUTS\"]]}]";
        let secret = Secret::from_str(s).unwrap();
        println!("{:?}", secret.as_bytes());

        let hasher = Sha256Hash::hash(secret.as_bytes()).to_byte_array();

        let expected_hash: [u8; 32] =
            hex::decode("b43b79ed408d4cc0aa75ad0a97ab21e357ff7ee027300fb573833c568431e808")
                .unwrap()
                .try_into()
                .unwrap();

        assert_eq!(hasher, expected_hash)
    }

    #[test]
    fn test_sct_root() {
        let s1: [u8; 32] =
            hex::decode("b43b79ed408d4cc0aa75ad0a97ab21e357ff7ee027300fb573833c568431e808")
                .unwrap()
                .try_into()
                .unwrap();
        let s2: [u8; 32] =
            hex::decode("6bad0d7d596cb9048754ee75daf13ee7e204c6e408b83ee67514369e3f8f3f96")
                .unwrap()
                .try_into()
                .unwrap();
        let s3: [u8; 32] =
            hex::decode("8da10ed117cad5e89c6131198ffe271166d68dff9ce961ff117bd84297133b77")
                .unwrap()
                .try_into()
                .unwrap();
        let s4: [u8; 32] =
            hex::decode("7ec5a236d308d2c2bf800d81d3e3df89cc98f4f937d0788c302d2754ba28166a")
                .unwrap()
                .try_into()
                .unwrap();
        let s5: [u8; 32] =
            hex::decode("e19353a94d1aaf56b150b1399b33cd4ef4096b086665945fbe96bd72c22097a7")
                .unwrap()
                .try_into()
                .unwrap();
        let s6: [u8; 32] =
            hex::decode("cc655b7103c8b999b3fc292484bcb5a526e2d0cbf951f17fd7670fc05b1ff947")
                .unwrap()
                .try_into()
                .unwrap();
        let s7: [u8; 32] =
            hex::decode("009ea9fae527f7914096da1f1ce2480d2e4cfea62480afb88da9219f1c09767f")
                .unwrap()
                .try_into()
                .unwrap();

        let leaf_hashes = &[s1, s2, s3, s4, s5, s6, s7];

        let root = merkle_root(leaf_hashes);

        let expected_root: [u8; 32] =
            hex::decode("71655cac0c83c6949169bcd6c82b309810138895f83b967089ffd9f64d109306")
                .unwrap()
                .try_into()
                .unwrap();

        assert_eq!(root, expected_root);
    }

    #[test]
    fn test_basic_merkle_proof() {
        // Test merkle proof for tree with two nodes.  Proof should be other hash.
        let hash1: [u8; 32] = [9; 32];
        let hash2: [u8; 32] = [8; 32];
        let leaf_hashes = vec![hash1, hash2];

        let position = 0;
        let proof = merkle_prove(leaf_hashes.clone(), position);
        let expected_proof = vec![hash2];
        assert_eq!(proof, expected_proof);

        let position = 1;
        let proof = merkle_prove(leaf_hashes.clone(), position);
        let expected_proof = vec![hash1];
        assert_eq!(proof, expected_proof);

        let proof = proof
            .iter()
            .map(|h| hex::encode(h))
            .collect::<Vec<String>>();

        let root = merkle_root(&leaf_hashes);

        let valid = merkle_verify(&root, &leaf_hashes[1], &proof);
        assert!(valid);
    }

    #[test]
    fn test_complex_merkle_proof() {
        let s1: [u8; 32] =
            hex::decode("b43b79ed408d4cc0aa75ad0a97ab21e357ff7ee027300fb573833c568431e808")
                .unwrap()
                .try_into()
                .unwrap();
        let s2: [u8; 32] =
            hex::decode("6bad0d7d596cb9048754ee75daf13ee7e204c6e408b83ee67514369e3f8f3f96")
                .unwrap()
                .try_into()
                .unwrap();
        let s3: [u8; 32] =
            hex::decode("8da10ed117cad5e89c6131198ffe271166d68dff9ce961ff117bd84297133b77")
                .unwrap()
                .try_into()
                .unwrap();
        let s4: [u8; 32] =
            hex::decode("7ec5a236d308d2c2bf800d81d3e3df89cc98f4f937d0788c302d2754ba28166a")
                .unwrap()
                .try_into()
                .unwrap();
        let s5: [u8; 32] =
            hex::decode("e19353a94d1aaf56b150b1399b33cd4ef4096b086665945fbe96bd72c22097a7")
                .unwrap()
                .try_into()
                .unwrap();
        let s6: [u8; 32] =
            hex::decode("cc655b7103c8b999b3fc292484bcb5a526e2d0cbf951f17fd7670fc05b1ff947")
                .unwrap()
                .try_into()
                .unwrap();
        let s7: [u8; 32] =
            hex::decode("009ea9fae527f7914096da1f1ce2480d2e4cfea62480afb88da9219f1c09767f")
                .unwrap()
                .try_into()
                .unwrap();

        let s8: [u8; 32] =
            hex::decode("7a56977edf9c299c1cfb14dfbeb2ab28d7b3d44b3c9cc6b7854f8a58acb3407d")
                .unwrap()
                .try_into()
                .unwrap();
        let s9: [u8; 32] =
            hex::decode("7de4c7c75c8082ed9a2124ce8f027ed9a60f2236b6f50c62748a220086ed367b")
                .unwrap()
                .try_into()
                .unwrap();

        let s10: [u8; 32] =
            hex::decode("b43b79ed408d4cc0aa75ad0a97ab21e357ff7ee027300fb573833c568431e808")
                .unwrap()
                .try_into()
                .unwrap();
        let s11: [u8; 32] =
            hex::decode("7de4c7c75c8082ed9a2124ce8f027ed9a60f2236b6f50c62748a220086ed367b")
                .unwrap()
                .try_into()
                .unwrap();

        let leaf_hashes = &[s1, s2, s3, s4, s5, s6, s7];

        let position = 0;
        let proofs = merkle_prove(leaf_hashes.to_vec(), position);
        let expected_proofs = [s8, s9].to_vec();
        assert_eq!(proofs, expected_proofs);

        let position = 1;
        let expected_proofs = [s3, s10, s11];
        let proofs = merkle_prove(leaf_hashes.to_vec(), position);
        assert_eq!(proofs, expected_proofs);

        let position = 2;
        let expected_proofs = [s2, s10, s11];
        let proofs = merkle_prove(leaf_hashes.to_vec(), position);
        assert_eq!(proofs, expected_proofs);
        assert_eq!(proofs, expected_proofs);

        assert_eq!(proofs, expected_proofs);
    }
}
