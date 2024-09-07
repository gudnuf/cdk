use core::panic;
use std::collections::HashMap;
use std::io::{self, stdin, Write};
use std::time::Duration;

use anyhow::{Error, Result};
use cdk::amount::{Amount, SplitTarget};
use cdk::nuts::nutdlc::{DLCLeaf, DLCRoot, DLCTimeoutLeaf, PayoutStructure};
use cdk::nuts::{self, , TokenV3};
use cdk::secret;
use cdk::url::UncheckedUrl;
use cdk::wallet::Wallet;
use clap::{Args, Subcommand};
use dlc::secp256k1_zkp::hashes::sha256;
use dlc::{
    secp256k1_zkp::{All, Secp256k1},
    OracleInfo,
};
use dlc_messages::oracle_msgs::{EventDescriptor, OracleAnnouncement};
use nostr_sdk::{
    hashes::hex::{Case, DisplayHex},
    Client, EventId, Keys, PublicKey, SecretKey,
};
use rand::rngs::ThreadRng;
use schnorr_fun::adaptor::{EncryptedSign, EncryptedSignature};
use schnorr_fun::fun::marker::{EvenY, NonZero, Normal, Public};
use schnorr_fun::fun::{KeyPair, Point};
use schnorr_fun::nonce::{GlobalRng, Synthetic};
use schnorr_fun::{fun::Scalar, Message, Schnorr};
use serde::{Deserialize, Serialize};

use sha2::Sha256;

use super::balance::mint_balances;

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
        amount: u64,
        //needs to show user outcomes an let user decide which outcome he wants
    },
    ListOffers {
        key: String,
    },
    DeleteOffers {
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
    timeout: u64,
    amount: u64,
    locked_ecash: Option<Vec<TokenV3>>,

    payoutstructs: Vec<PayoutStructure>, // user_a dlc funding proofs
                                         // What other data needs to be passed around to create the contract?
}

/// To manage DLC contracts (ie. creating and accepting bets)
// TODO: Different name?
// TODO: put the wallet in here instead of passing it in every function
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

    async fn create_funding_token(
        &self,
        wallet: &Wallet,
        dlc_root: &DLCRoot,
        amount: u64,
    ) -> Result<(TokenV3, secret::Secret), Error> {
        let threshold = 1; // TOOD: this should come from payout structures
        let dlc_conditions =
            nuts::nut11::Conditions::new(None, None, None, None, None, Some(threshold))?;

        let dlc_secret =
            nuts::nut10::Secret::new(nuts::Kind::DLC, dlc_root.to_string(), Some(dlc_conditions));
        let dlc_secret = dlc_secret.try_into()?;
        // TODO: this will put the same secret into each proof.
        // I'm not sure if the mint will allow us to spend multiple proofs with the same backup secret
        // If not, we can use a p2pk backup, or new backup secret for each proof
        let backup_secret = secret::Secret::generate();

        //
        let sct_root = nuts::nutsct::sct_root(vec![dlc_secret, backup_secret.clone()]);

        let sct_conditions = nuts::nut11::SpendingConditions::new_sct(sct_root);

        let available_proofs = wallet.get_proofs().await?;

        let include_fees = false;

        let selected = wallet
            .select_proofs_to_send(Amount::from(amount), available_proofs, include_fees)
            .await
            .unwrap();
        let funding_proofs = wallet
            .swap(
                Some(Amount::from(amount)),
                SplitTarget::default(),
                selected,
                Some(sct_conditions),
                include_fees,
            )
            .await?
            .unwrap();

        // TODO: encode this as a Token

        let token = cdk::nuts::nut00::TokenV3::new(
            UncheckedUrl::from("https://testnut.cashu.space"),
            funding_proofs.clone(),
            Some(String::from("dlc locking proofs")),
            None,
        )?;

        println!(
            "Funding proof secrets: {:?}",
            funding_proofs
                .iter()
                .map(|p| p.secret.to_string())
                .collect::<Vec<String>>()
        );

        Ok((token, backup_secret))
    }

    /// Start a new DLC contract, and send to the counterparty
    /// # Arguments
    /// * `announcement` - OracleAnnouncement
    /// * `announcement_id` - Id of kind 88 event
    /// * `counterparty_pubkey` - hex encoded public key of counterparty
    /// * `outcomes` - ??outcomes this user wants to bet on?? I think!
    pub async fn create_bet(
        &self,
        wallet: &Wallet,
        announcement: OracleAnnouncement,
        announcement_id: EventId,
        counterparty_pubkey: nostr_sdk::key::PublicKey,
        outcomes: Vec<String>,
        amount: u64,
    ) -> Result<EventId, Error> {
        // TODO:: figure out how to pass XOnlyPublicKey to PayoutStructure
        let winning_payout_structure = PayoutStructure::default(self.keys.public_key().to_string());
        let winning_counterparty_payout_structure =
            PayoutStructure::default(counterparty_pubkey.to_string());
        // timeout set to 1 hour from event_maturity_epoch
        let timeout = (announcement.oracle_event.event_maturity_epoch as u64)
            + Duration::from_secs(60 * 60).as_secs();
        let timeout_payout_structure = PayoutStructure::default_timeout(vec![
            self.keys.public_key().to_string(),
            counterparty_pubkey.to_string(),
        ]);

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

        if all_outcomes.len() < outcomes.len() {
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
        let leaf_hashes: Vec<DLCLeaf> = blinded_adaptor_points
            .iter()
            .map(|(outcome, point)| {
                if outcomes.contains(outcome) {
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
                }
            })
            .collect();

        // Add timeout leaf
        let timeout_leaf = DLCTimeoutLeaf::new(&timeout, &timeout_payout_structure);
        let dlc_root = DLCRoot::compute(leaf_hashes, Some(timeout_leaf));

        // TODO: not sure this is what we want to do here
        let sigs: HashMap<String, EncryptedSignature> = blinded_adaptor_points
            .into_iter()
            .map(|(outcome, point)| {
                // TODO: figure out what the message is... Is this the payout structure?
                let sig = self.adaptor_sign(point.serialize(), outcome.clone());
                Ok((outcome, sig))
            })
            .collect::<Result<_, Error>>()?;

        let (token, backup_secret) = self
            .create_funding_token(&wallet, &dlc_root, amount)
            .await?;

        // TODO: backup the backup secret

        let offer_dlc = UserBet {
            id: 7, // TODO,
            oracle_announcement: announcement.clone(),
            oracle_event_id: announcement_id.to_string(),
            user_outcomes: outcomes,
            blinding_factor: blinding_factor.to_be_bytes().to_hex_string(Case::Lower),
            dlc_root: dlc_root.to_string(),
            timeout,
            amount,
            locked_ecash: Some(vec![token]),
            payoutstructs: vec![
                winning_payout_structure,
                winning_counterparty_payout_structure,
            ],
        };

        let offer_dlc = serde_json::to_string(&offer_dlc)?;

        println!("{:?}", offer_dlc);

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

pub async fn dlc(
    wallets: HashMap<UncheckedUrl, Wallet>,
    sub_command_args: &DLCSubCommand,
) -> Result<()> {
    //let keys =
    //   Keys::parse("nsec15jldh0htg2qeeqmqd628js8386fu4xwpnuqddacc64gh0ezdum6qaw574p").unwrap();

    match &sub_command_args.command {
        DLCCommands::CreateBet {
            key,
            oracle_event_id,
            counterparty_pubkey,
            amount,
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

            for (i, outcome) in outcomes.clone().into_iter().enumerate() {
                println!("outcome {i}: {outcome}");
            }

            let mut input_line = String::new();

            println!("please select outcome by number");

            stdin()
                .read_line(&mut input_line)
                .expect("Failed to read line");
            let choice: i32 = input_line.trim().parse().expect("Input not an integer");

            let outcome_choice = vec![outcomes[choice as usize].clone()];

            println!(
                "You chose outcome {:?} to bet {} on",
                outcome_choice, amount
            );

            /* let user pick which wallet to use */
            let mints_amounts = mint_balances(wallets).await?;

            println!("Enter a mint number to create a DLC offer for");

            let mut user_input = String::new();
            io::stdout().flush().unwrap();
            stdin().read_line(&mut user_input)?;

            let mint_number: usize = user_input.trim().parse()?;

            if mint_number.gt(&(mints_amounts.len() - 1)) {
                crate::bail!("Invalid mint number");
            }

            let wallet = mints_amounts[mint_number].0.clone();

            let event_id = dlc
                .create_bet(
                    &wallet,
                    oracle_announcement,
                    oracle_event_id,
                    counterparty_pubkey,
                    outcomes,
                    *amount,
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
        DLCCommands::DeleteOffers { key } => {
            let keys = Keys::parse(key).unwrap();

            let dlc = DLC::new(keys.secret_key()?).await?;

            let bets = nostr_events::delete_all_dlc_offers(&keys, &dlc.nostr).await;

            println!("{:?}", bets);
        }
        _ => todo!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, str::FromStr, sync::Arc};

    use bip39::Mnemonic;
    use cdk::{
        cdk_database::{self, WalletDatabase},
        url::UncheckedUrl,
        wallet::Wallet,
    };
    use cdk_sqlite::WalletSqliteDatabase;
    use dlc_messages::oracle_msgs::EventDescriptor;
    use nostr_sdk::{Client, EventId, Keys};
    use rand::Rng;

    use crate::sub_commands::dlc::{
        nostr_events::{delete_all_dlc_offers, list_dlc_offers},
        utils::oracle_announcement_from_str,
        DLC,
    };

    const DEFAULT_WORK_DIR: &str = ".cdk-cli";
    const MINT_URL: &str = "http://localhost:3338";

    /// helper function to initialize wallets
    async fn initialize_wallets() -> HashMap<UncheckedUrl, Wallet> {
        let work_dir = {
            let home_dir = home::home_dir().unwrap();
            home_dir.join(DEFAULT_WORK_DIR)
        };
        let localstore: Arc<dyn WalletDatabase<Err = cdk_database::Error> + Send + Sync> = {
            let sql_path = work_dir.join("cdk-cli.sqlite");
            let sql = WalletSqliteDatabase::new(&sql_path).await.unwrap();

            sql.migrate().await;

            Arc::new(sql)
        };

        let seed_path = work_dir.join("seed");

        let mnemonic = match fs::metadata(seed_path.clone()) {
            Ok(_) => {
                let contents = fs::read_to_string(seed_path.clone()).unwrap();
                Mnemonic::from_str(&contents).unwrap()
            }
            Err(_e) => {
                let mut rng = rand::thread_rng();
                let random_bytes: [u8; 32] = rng.gen();

                let mnemnic = Mnemonic::from_entropy(&random_bytes).unwrap();
                tracing::info!("Using randomly generated seed you will not be able to restore");

                mnemnic
            }
        };

        let mut wallets: HashMap<UncheckedUrl, Wallet> = HashMap::new();

        let mints = localstore.get_mints().await.unwrap();

        for (mint, _) in mints {
            let wallet = Wallet::new(
                &mint.to_string(),
                cdk::nuts::CurrencyUnit::Sat,
                localstore.clone(),
                &mnemonic.to_seed_normalized(""),
                None,
            );

            wallets.insert(mint, wallet);
        }
        wallets
    }

    #[tokio::test]
    async fn test_create_and_post_offer() {
        let wallets = initialize_wallets().await;
        let wallet = match wallets.get(&UncheckedUrl::new(MINT_URL.to_string())) {
            Some(wallet) => wallet.clone(),
            None => todo!(),
        };
        const ANNOUNCEMENT: &str = "ypyyyX6pdZUM+OovHftxK9StImd8F7nxmr/eTeyR/5koOVVe/EaNw1MAeJm8LKDV1w74Fr+UJ+83bVP3ynNmjwKbtJr9eP5ie2Exmeod7kw4uNsuXcw6tqJF1FXH3fTF/dgiOwAByEOAEd95715DKrSLVdN/7cGtOlSRTQ0/LsW/p3BiVOdlpccA/dgGDAACBDEyMzQENDU2NwR0ZXN0";
        let announcement = oracle_announcement_from_str(ANNOUNCEMENT);
        let announcement_id =
            EventId::from_hex("d30e6c857a900ebefbf7dc3b678ead9215f4345476067e146ded973971286529")
                .unwrap();
        let keys = Keys::generate();
        let counterparty_keys = Keys::generate();

        let dlc = DLC::new(&keys.secret_key().unwrap()).await.unwrap();

        let descriptor = &announcement.oracle_event.event_descriptor;

        let outcomes = match descriptor {
            EventDescriptor::EnumEvent(ref e) => e.outcomes.clone(),
            EventDescriptor::DigitDecompositionEvent(_) => unreachable!(),
        };
        let outcome1 = &outcomes.clone()[0];

        let amount = 7;
        let _event_id = dlc
            .create_bet(
                &wallet,
                announcement,
                announcement_id,
                counterparty_keys.public_key(),
                vec![outcome1.clone()],
                amount,
            )
            .await
            .unwrap();

        let client = Client::new(&Keys::generate());
        let relay = "wss://relay.damus.io";
        client.add_relay(relay.to_string()).await.unwrap();
        client.connect().await;

        let offers = list_dlc_offers(&counterparty_keys, &client) // error line 74:58 in nostr_events.rs
            .await
            .unwrap(); // if event exists should unwrap to event

        println!("{:?}", offers);

        assert!(offers.len() >= 1);

        /* clean up */
        delete_all_dlc_offers(&keys, &client).await;
    }
}

// ALICE:
// - pub: d71b2434429b0f038ed35e0e3827bca5e65b6d44d1af9344f73b20ff7ffa93dd
// - priv: b9452287c9e4cf53cf935adbc2341931c68c19d8447fe571ccc8dd9b5ed85584
// BOB:
// - pub: b3e6ae1bdfa18106dafe4992b77149a38623662f78f5f60ee436e457f7965ee2
// - priv: 4e111131d31ad92ed5d37ab87d5046efa730f192f9c8f9b59f6c61caad1f8933

// anouncement_ID: d30e6c857a900ebefbf7dc3b678ead9215f4345476067e146ded973971286529
