use core::panic;
use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Error, Result};
use cdk::nuts;
use cdk::nuts::nutdlc::{DLCLeaf, PayoutStructure};
use clap::{Args, Subcommand};
use dlc::secp256k1_zkp::hashes::sha256;
use dlc::{
    secp256k1_zkp::{All, Secp256k1},
    OracleInfo,
};
use dlc_messages::oracle_msgs::{EventDescriptor, OracleAnnouncement};
use lightning::util::ser::Writeable;
use nostr_sdk::{
    hashes::hex::{Case, DisplayHex},
    Client, EventId, Keys, PublicKey, SecretKey,
};
use rand::rngs::ThreadRng;
use schnorr_fun::adaptor::{EncryptedSign, EncryptedSignature};
use schnorr_fun::fun::marker::{EvenY, NonZero, Normal, Public};
use schnorr_fun::fun::{KeyPair, Point};
use schnorr_fun::nonce::{GlobalRng, Synthetic};
use schnorr_fun::{fun::Scalar, Message, Schnorr, Signature};
use serde::{Deserialize, Serialize};

use sha2::Sha256;

pub mod nostr_events;
pub mod utils;

const RELAYS: [&str; 1] = ["wss://relay.damus.io"];

#[derive(Args)]
pub struct DLCSubCommand {
    #[command(subcommand)]
    pub command: DLCCommands,
}

#[derive(Subcommand)]
pub enum DLCCommands {
    CreateBet {
        key: String,
        oracle_event_id: String,
        counterparty_pubkey: String,
    },
    ListOffers {
        key: String,
    }, // AcceptBet {
       //     // the event id of the offered bet
       //     event_id: String,
       // },
}

// I imagine this is what will be sent back and forth in the kind 8888 messages
#[derive(Serialize, Deserialize, Debug)]
pub struct UserBet {
    pub id: i32,
    pub oracle_announcement: OracleAnnouncement,
    oracle_event_id: String,
    user_outcomes: Vec<String>,
    blinding_factor: String,
    dlc_root: String,
    // user_a dlc funding proofs
    // What other data needs to be passed around to create the contract?
}

/// To manage DLC contracts (ie. creating and accepting bets)
// TODO: Different name?
pub struct DLC {
    keys: Keys,
    nostr: Client,
    signing_keypair: KeyPair<EvenY>,
    schnorr: Schnorr<Sha256, Synthetic<Sha256, GlobalRng<ThreadRng>>>,
    secp: Secp256k1<All>,
}

impl DLC {
    /// Create new [`DLC`]
    pub async fn new(secret_key: &SecretKey) -> Result<Self, Error> {
        let keys = Keys::parse(secret_key.to_string())?;
        let nostr = Client::new(&keys.clone());
        for relay in RELAYS.iter() {
            nostr.add_relay(relay.to_string()).await?;
        }
        nostr.connect().await;
        let nonce_gen = Synthetic::<Sha256, GlobalRng<ThreadRng>>::default();
        let schnorr = Schnorr::<Sha256, _>::new(nonce_gen);
        let scalar = Scalar::from_bytes(secret_key.secret_bytes())
            .ok_or(Error::msg("Invalid secret key"))?;
        let scalar = scalar.non_zero().ok_or(Error::msg("Invalid secret key"))?;
        let signing_keypair = schnorr.new_keypair(scalar);
        let secp: Secp256k1<All> = Secp256k1::gen_new();

        Ok(Self {
            keys,
            nostr,
            signing_keypair,
            schnorr,
            secp,
        })
    }

    fn adaptor_sign(&self, encryption_key: [u8; 33], message: String) -> EncryptedSignature {
        let encryption_key: Point<Normal, Public, NonZero> =
            Point::from_bytes(encryption_key).expect("Valid pubkey");
        let message = Message::<Public>::raw(message.as_bytes());

        self.schnorr
            .encrypted_sign(&self.signing_keypair, &encryption_key, message)
    }

    /// Start a new DLC contract, and send to the counterparty
    /// # Arguments
    /// * `announcement` - OracleAnnouncement
    /// * `announcement_id` - Id of kind 88 event
    /// * `counterparty_pubkey` - hex encoded public key of counterparty
    /// * `outcomes` - ??outcomes this user wants to bet on?? I think!
    pub async fn create_bet(
        &self,
        announcement: OracleAnnouncement,
        announcement_id: EventId,
        counterparty_pubkey: nostr_sdk::key::PublicKey,
        outcomes: Vec<String>,
    ) -> Result<EventId, Error> {
        // TODO:: figure out how to pass XOnlyPublicKey to PayoutStructure
        let winning_payout_structure = PayoutStructure::default(self.keys.public_key().to_string());
        let winning_counterparty_payout_structure =
            PayoutStructure::default(counterparty_pubkey.to_string());
        // TODO: create blinded outcome locking points
        // TODO: create dlc funding proofs

        let oracle_info = OracleInfo {
            public_key: announcement.oracle_public_key,
            nonces: announcement.oracle_event.oracle_nonces.clone(),
        };

        // get all event outcomes from the announcement
        let all_outcomes = if let EventDescriptor::EnumEvent(ref desc) =
            announcement.oracle_event.event_descriptor
        {
            if !outcomes.iter().all(|o| desc.outcomes.contains(o)) {
                return Err(Error::msg("Invalid outcomes"));
            }
            desc.outcomes.clone()
        } else {
            return Err(Error::msg("Invalid event descriptor"));
        };

        if all_outcomes.len() != outcomes.len() {
            return Err(Error::msg(
                "Input outcomes should be a subset of all outcomes",
            ));
        }

        let blinding_factor = cdk::secp256k1::Scalar::random();

        let blinded_adaptor_points: HashMap<String, dlc::secp256k1_zkp::PublicKey> = all_outcomes
            .into_iter()
            .map(|outcome| {
                // hash the outcome
                let msg = vec![
                    dlc::secp256k1_zkp::Message::from_hashed_data::<sha256::Hash>(
                        outcome.as_bytes(),
                    ),
                ];

                // get adaptor point
                let point = dlc::get_adaptor_point_from_oracle_info(
                    &self.secp,
                    &[oracle_info.clone()],
                    &[msg],
                )
                .unwrap();

                // blind adaptor point with Ki_ = Ki + b * G
                let point = point.add_exp_tweak(&self.secp, &blinding_factor).unwrap();

                // TODO: figure out what the message is... Is this the payout structure defined as `xonly_pubkey -> weight`?
                // let sig = self.adaptor_sign(point.serialize(), "fix me".to_string());

                Ok((outcome, point))
            })
            .collect::<Result<_, Error>>()?;

        // create leaf hashes for each outcome
        let leaf_hashes: Vec<[u8; 32]> = blinded_adaptor_points
            .iter()
            .map(|(outcome, point)| {
                let leaf = if outcomes.contains(outcome) {
                    // we win
                    DLCLeaf {
                        blinded_locking_point: cdk::nuts::PublicKey::from_slice(&point.serialize())
                            .expect("valid public key"),
                        payout: winning_payout_structure.clone(),
                    }
                } else {
                    // they win
                    DLCLeaf {
                        blinded_locking_point: cdk::nuts::PublicKey::from_slice(&point.serialize())
                            .expect("valid public key"),
                        payout: winning_counterparty_payout_structure.clone(),
                    }
                };
                leaf.hash()
            })
            .collect();

        // TODO: Timeoute leaf

        let merkle_root = nuts::nutsct::merkle_root(&leaf_hashes);

        // TODO: not sure this is what we want to do here
        let sigs: HashMap<String, EncryptedSignature> = blinded_adaptor_points
            .into_iter()
            .map(|(outcome, point)| {
                // TODO: figure out what the message is... Is this the payout structure?
                let sig = self.adaptor_sign(point.serialize(), outcome.clone());
                Ok((outcome, sig))
            })
            .collect::<Result<_, Error>>()?;

        let offer_dlc = UserBet {
            id: 0, // TODO,
            oracle_announcement: announcement.clone(),
            oracle_event_id: announcement_id.to_string(),
            user_outcomes: outcomes,
            blinding_factor: blinding_factor.to_be_bytes().to_hex_string(Case::Lower),
            dlc_root: merkle_root.to_hex_string(Case::Lower),
        };

        let offer_dlc = serde_json::to_string(&offer_dlc)?;

        let offer_dlc_event =
            nostr_events::create_dlc_msg_event(&self.keys, offer_dlc, &counterparty_pubkey)?;

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

            let dlc = DLC::new(keys.secret_key()?).await?;

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

            let event_id = dlc
                .create_bet(
                    oracle_announcement,
                    oracle_event_id,
                    counterparty_pubkey,
                    outcomes,
                )
                .await?;

            println!("Event {} sent to {}", event_id, counterparty_pubkey);
        }
        DLCCommands::ListOffers { key } => {
            let keys = Keys::parse(key).unwrap();

            let dlc = DLC::new(keys.secret_key()?).await?;

            let bets = nostr_events::list_dlc_offers(&keys, &dlc.nostr).await;

            println!("{:?}", bets);
        }
        _ => todo!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nostr_sdk::Keys;

    #[test]
    fn generate_nostr_key() {
        let keys = Keys::generate();
        println!("{}", keys.public_key());
        println!("{}", keys.secret_key().unwrap());
    }
}

// ALICE:
// - pub: d71b2434429b0f038ed35e0e3827bca5e65b6d44d1af9344f73b20ff7ffa93dd
// - priv: b9452287c9e4cf53cf935adbc2341931c68c19d8447fe571ccc8dd9b5ed85584
// BOB:
// - pub: b3e6ae1bdfa18106dafe4992b77149a38623662f78f5f60ee436e457f7965ee2
// - priv: 4e111131d31ad92ed5d37ab87d5046efa730f192f9c8f9b59f6c61caad1f8933

// anouncement_ID: d30e6c857a900ebefbf7dc3b678ead9215f4345476067e146ded973971286529
