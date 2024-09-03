use core::panic;

use anyhow::{Error, Result};
use clap::{Args, Subcommand};
use dlc_messages::oracle_msgs::{EventDescriptor, OracleAnnouncement};

use dlc::secp_utils::schnorrsig_compute_sig_point;
use nostr_sdk::{
    bitcoin::{PrivateKey, XOnlyPublicKey},
    secp256k1::{self, SecretKey},
    Client, EventId, Keys, PublicKey,
};

use bitcoin::secp256k1::KeyPair;

use cdk::{dhke::hash_to_curve, secp256k1::{Message, PublicKey as cdkPublicKey, Secp256k1}};

use sha2::{Digest, Sha256};

pub mod nostr_events;
pub mod utils;

const RELAYS: [&str; 1] = ["wss://relay.damus.io"];

#[derive(Args)]
pub struct DLCSubCommand {
    #[command(subcommand)]
    pub command: DLCCommands,
}


pub struct PayoutStructure {
    public_payout_hash: Vec<cdk::nuts::PublicKey>, //paper says payout hash, nut says xonly_pubkey, return value from hash_to_curve is cdk::nuts::PublicKey
    weight: Vec<u8>,
}

impl PayoutStructure{
    // verify there is a weight for each public key
    pub async fn verify(&self) -> bool {
        self.public_payout_hash.len() == self.weight.len()
    }
}
pub struct Branch {
    blinded_locking_point: cdk::nuts::PublicKey,
    payout_structure: PayoutStructure,
}


#[derive(Subcommand)]
pub enum DLCCommands {
    CreateBet {
        key: String,
        oracle_event_id: String,
        counterparty_pubkey: String,
    },
    // AcceptBet {
    //     // the event id of the offered bet
    //     event_id: String,
    // },
}

// I imagine this is what will be sent back and forth in the kind 8888 messages
pub struct UserBet {
    pub id: i32,
    pub oracle_announcement: OracleAnnouncement,
    oracle_event_id: String,
    user_outcomes: Vec<String>,
    // blinding factor used to create blinded outcome locking points
    // user_a dlc funding proofs
    // What other data needs to be passed around to create the contract?

    // Timeout branch??
    pub blinding_factor: KeyPair,
    pub payout_strucures: Vec<PayoutStructure>,
    pub merkle_root_hash: [u8; 32],
    pub ecash_note: u32, //Option( vec!(ecash notes))  I dont know the struct for an ecash note

    pub timeout: u32,
}

impl UserBet {
    pub async fn locking_points(&self) -> Vec<cdkPublicKey> {
        // oracle announcment.nonce + blinding_factor * G
        let mut locking_points: Vec<cdkPublicKey> = Vec::new();
        let oracle_public_key = self.oracle_announcement.oracle_public_key;

        let nonce = self.oracle_announcement.oracle_event.oracle_nonces[0];

        match &self.oracle_announcement.oracle_event.event_descriptor {
            EventDescriptor::EnumEvent(event) => {
                for outcome in &event.outcomes {
                    let mut hasher = Sha256::new();
                    hasher.update(outcome.as_bytes());
                    let result = hasher.finalize();
                    let message = Message::from_slice(&result).expect("error caclulating message");
                    
                    //https://adiabat.github.io/dlc.pdf
                    // https://github.com/p2pderivatives/rust-dlc/blob/master/dlc/src/secp_utils.rs
                    // let sG = R - h(m, R)A
                    // let sG = Nonce point - sha256(message || nonce) * OraclePubKey
                    // lockingpoints.push(sG)
                    // s is what the oracle posts in the attestation message so user can spend coins
                    let secp = Secp256k1::new();
                    let sig_point =
                        schnorrsig_compute_sig_point(&secp, &oracle_public_key, &nonce, &message)
                            .expect("error calculating signature point");
                    locking_points.push(sig_point);
                }
            }
            EventDescriptor::DigitDecompositionEvent(_) => todo!(),
        }
        locking_points
    }

    // Todo create blinded locking points and payout branches from them.

    pub async fn timeout_branch(&self) -> Branch {
        let timeout_hash = hash_to_curve(&self.timeout.to_be_bytes()).expect("error calculating timeout hash");

        let payout_structure = PayoutStructure {
            public_payout_hash: vec!(cdk::nuts::PublicKey::from_slice(&[0; 32]).unwrap()), 
            weight: vec!(1),
        };

        Branch {
            blinded_locking_point: timeout_hash,
            payout_structure,
        }
    }
}

/// To manage DLC contracts (ie. creating and accepting bets)
// TODO: Different name?
pub struct DLC {
    keys: Keys,
    nostr: Client,
}

impl DLC {
    /// Create new [`DLC`]
    pub async fn new(keys: Keys) -> Result<Self, Error> {
        let nostr = Client::new(&keys);
        for relay in RELAYS.iter() {
            nostr.add_relay(relay.to_string()).await?;
        }
        nostr.connect().await;

        Ok(Self { keys, nostr })
    }

    /// Start a new DLC contract, and send to the counterparty
    pub async fn create_bet(
        &self,
        announcement: OracleAnnouncement,
        announcement_id: EventId,
        counterparty_pubkey: nostr_sdk::key::PublicKey,
        outcomes: Vec<String>,
    ) -> Result<EventId, Error> {
        // TODO: create blinded outcome locking points
        // TODO: create dlc funding proofs

        let msg = todo!("Create a user bet message and serialize it");

        let offer_dlc_event =
            nostr_events::create_dlc_msg_event(&self.keys, msg, &counterparty_pubkey)?;

        match self.nostr.send_event(offer_dlc_event).await {
            Ok(event_id) => Ok(event_id),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub async fn accept_bet(&self, event_id: EventId) -> Result<EventId, Error> {
        todo!()
    }
}

pub async fn dlc(sub_command_args: &DLCSubCommand) -> Result<()> {
    //let keys =
    //   Keys::parse("nsec15jldh0htg2qeeqmqd628js8386fu4xwpnuqddacc64gh0ezdum6qaw574p").unwrap();

    match &sub_command_args.command {
        DLCCommands::CreateBet {
            key,
            oracle_event_id,
            counterparty_pubkey,
        } => {
            let keys = Keys::parse(key).unwrap();
            let oracle_event_id = EventId::from_hex(oracle_event_id).unwrap();
            let counterparty_pubkey = PublicKey::from_hex(counterparty_pubkey).unwrap();

            let dlc = DLC::new(keys).await?;

            let announcement_event =
                match nostr_events::lookup_announcement_event(oracle_event_id, &dlc.nostr).await {
                    Some(Ok(event)) => event,
                    _ => panic!("Oracle announcement event not found"),
                };

            let oracle_announcement =
                utils::oracle_announcement_from_str(&announcement_event.content);

            println!(
                "Oracle announcement event content: {:?}",
                oracle_announcement
            );

            // // TODO: get the outcomes from the oracle announcement???
            let outcomes = match oracle_announcement.oracle_event.event_descriptor {
                EventDescriptor::EnumEvent(ref e) => e.outcomes.clone(),
                EventDescriptor::DigitDecompositionEvent(_) => unreachable!(),
            };

            println!("Outcomes: {:?}", outcomes);

            // let event_id = dlc
            //     .create_bet(
            //         oracle_announcement,
            //         *oracle_event_id,
            //         *counterparty_pubkey,
            //         outcomes,
            //     )
            //     .await?;

            // println!("Event {} sent to {}", event_id, counterparty_pubkey);
        }
        _ => todo!(),
    }
    Ok(())
}
