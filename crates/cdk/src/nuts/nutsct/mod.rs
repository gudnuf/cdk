use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::hashes::Hash;

use crate::secret::Secret;

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
pub fn merkle_verify(root: &[u8; 32], leaf_hash: &[u8; 32], proof: &[&[u8; 32]]) -> bool {
    let h = leaf_hash;
    for branch_hash in proof {
        let h = sorted_merkle_hash(h, *branch_hash);
    }

    return h == root;
}

pub fn sct_root(secrets: Vec<Secret>) -> [u8; 32] {
    let leaf_hashes: Vec<[u8; 32]> = secrets
        .iter()
        .map(|s| Sha256Hash::hash(&s.to_bytes()).to_byte_array())
        .collect();

    merkle_root(&leaf_hashes)
}
