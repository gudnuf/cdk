//! Client to connet to mint
use std::str::FromStr;

use cashu::nuts::nut00::wallet::BlindedMessages;
use cashu::nuts::nut00::{BlindedMessage, Proof};
use cashu::nuts::nut01::Keys;
use cashu::nuts::nut03::RequestMintResponse;
use cashu::nuts::nut04::{MintRequest, PostMintResponse};
use cashu::nuts::nut05::{CheckFeesRequest, CheckFeesResponse};
use cashu::nuts::nut06::{SplitRequest, SplitResponse};
#[cfg(feature = "nut07")]
use cashu::nuts::nut07::{CheckSpendableRequest, CheckSpendableResponse};
use cashu::nuts::nut08::{MeltRequest, MeltResponse};
#[cfg(feature = "nut09")]
use cashu::nuts::nut09::MintInfo;
use cashu::nuts::*;
use cashu::url::UncheckedUrl;
use cashu::{utils, Amount};
#[cfg(target_arch = "wasm32")]
use gloo::net::http::Request;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use url::Url;

#[cfg(feature = "blocking")]
pub mod blocking;

pub use cashu::Bolt11Invoice;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invoice not paid")]
    InvoiceNotPaid,
    #[error("Wallet not responding")]
    LightingWalletNotResponding(Option<String>),
    /// Parse Url Error
    #[error("`{0}`")]
    UrlParse(#[from] url::ParseError),
    /// Serde Json error
    #[error("`{0}`")]
    SerdeJson(#[from] serde_json::Error),
    /// Cashu Url Error
    #[error("`{0}`")]
    CashuUrl(#[from] cashu::url::Error),
    ///  Min req error
    #[cfg(not(target_arch = "wasm32"))]
    #[error("`{0}`")]
    MinReq(#[from] minreq::Error),
    #[cfg(target_arch = "wasm32")]
    #[error("`{0}`")]
    Gloo(String),
    /// Custom Error
    #[error("`{0}`")]
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintErrorResponse {
    code: u32,
    error: Option<String>,
    detail: Option<String>,
}

impl Error {
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let mint_res: MintErrorResponse = serde_json::from_str(json)?;

        let err = mint_res
            .error
            .as_deref()
            .or(mint_res.detail.as_deref())
            .unwrap_or_default();

        let mint_error = match err {
            error if error.starts_with("Lightning invoice not paid yet.") => Error::InvoiceNotPaid,
            error if error.starts_with("Lightning wallet not responding") => {
                let mint = utils::extract_url_from_error(error);
                Error::LightingWalletNotResponding(mint)
            }
            error => Error::Custom(error.to_owned()),
        };
        Ok(mint_error)
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub mint_url: UncheckedUrl,
}

impl Client {
    pub fn new(mint_url: &str) -> Result<Self, Error> {
        let mint_url = UncheckedUrl::from_str(mint_url)?;
        let _: Url = (&mint_url).try_into()?;
        Ok(Self { mint_url })
    }

    /// Get Mint Keys [NUT-01]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_keys(&self) -> Result<Keys, Error> {
        let url = self.mint_url.join("keys")?;
        let keys = minreq::get(url).send()?.json::<Value>()?;

        let keys: Keys = serde_json::from_str(&keys.to_string())?;
        Ok(keys)
    }

    /// Get Mint Keys [NUT-01]
    #[cfg(target_arch = "wasm32")]
    pub async fn get_keys(&self) -> Result<Keys, Error> {
        let url = self.mint_url.join("keys")?;
        let keys = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let keys: Keys = serde_json::from_str(&keys.to_string())?;
        /*
                let keys: BTreeMap<u64, String> = match serde_json::from_value(keys.clone()) {
                    Ok(keys) => keys,
                    Err(_err) => {
                        return Err(Error::CustomError(format!(
                            "url: {}, {}",
                            url,
                            serde_json::to_string(&keys)?
                        )))
                    }
                };

                let mint_keys: BTreeMap<u64, PublicKey> = keys
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let key = hex::decode(v).ok()?;
                        let public_key = PublicKey::from_sec1_bytes(&key).ok()?;
                        Some((k, public_key))
                    })
                    .collect();
        */
        Ok(keys)
    }

    /// Get Keysets [NUT-02]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_keysets(&self) -> Result<nut02::Response, Error> {
        let url = self.mint_url.join("keysets")?;
        let res = minreq::get(url).send()?.json::<Value>()?;

        let response: Result<nut02::Response, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Get Keysets [NUT-02]
    #[cfg(target_arch = "wasm32")]
    pub async fn get_keysets(&self) -> Result<nut02::Response, Error> {
        let url = self.mint_url.join("keysets")?;
        let res = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<nut02::Response, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Request Mint [NUT-03]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn request_mint(&self, amount: Amount) -> Result<RequestMintResponse, Error> {
        let mut url = self.mint_url.join("mint")?;
        url.query_pairs_mut()
            .append_pair("amount", &amount.to_sat().to_string());

        let res = minreq::get(url).send()?.json::<Value>()?;

        let response: Result<RequestMintResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Request Mint [NUT-03]
    #[cfg(target_arch = "wasm32")]
    pub async fn request_mint(&self, amount: Amount) -> Result<RequestMintResponse, Error> {
        let mut url = self.mint_url.join("mint")?;
        url.query_pairs_mut()
            .append_pair("amount", &amount.to_sat().to_string());

        let res = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<RequestMintResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Mint Tokens [NUT-04]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn mint(
        &self,
        blinded_messages: BlindedMessages,
        hash: &str,
    ) -> Result<PostMintResponse, Error> {
        let mut url = self.mint_url.join("mint")?;
        url.query_pairs_mut().append_pair("hash", hash);

        let request = MintRequest {
            outputs: blinded_messages.blinded_messages,
        };

        let res = minreq::post(url)
            .with_json(&request)?
            .send()?
            .json::<Value>()?;

        let response: Result<PostMintResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Mint Tokens [NUT-04]
    #[cfg(target_arch = "wasm32")]
    pub async fn mint(
        &self,
        blinded_messages: BlindedMessages,
        hash: &str,
    ) -> Result<PostMintResponse, Error> {
        let mut url = self.mint_url.join("mint")?;
        url.query_pairs_mut().append_pair("hash", hash);

        let request = MintRequest {
            outputs: blinded_messages.blinded_messages,
        };

        let res = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<PostMintResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Check Max expected fee [NUT-05]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn check_fees(&self, invoice: Bolt11Invoice) -> Result<CheckFeesResponse, Error> {
        let url = self.mint_url.join("checkfees")?;

        let request = CheckFeesRequest { pr: invoice };

        let res = minreq::post(url)
            .with_json(&request)?
            .send()?
            .json::<Value>()?;

        let response: Result<CheckFeesResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Check Max expected fee [NUT-05]
    #[cfg(target_arch = "wasm32")]
    pub async fn check_fees(&self, invoice: Bolt11Invoice) -> Result<CheckFeesResponse, Error> {
        let url = self.mint_url.join("checkfees")?;

        let request = CheckFeesRequest { pr: invoice };

        let res = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<CheckFeesResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Melt [NUT-05]
    /// [Nut-08] Lightning fee return if outputs defined
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn melt(
        &self,
        proofs: Vec<Proof>,
        invoice: Bolt11Invoice,
        outputs: Option<Vec<BlindedMessage>>,
    ) -> Result<MeltResponse, Error> {
        let url = self.mint_url.join("melt")?;

        let request = MeltRequest {
            proofs,
            pr: invoice,
            outputs,
        };

        let value = minreq::post(url)
            .with_json(&request)?
            .send()?
            .json::<Value>()?;

        let response: Result<MeltResponse, serde_json::Error> =
            serde_json::from_value(value.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&value.to_string())?),
        }
    }

    /// Melt [NUT-05]
    /// [Nut-08] Lightning fee return if outputs defined
    #[cfg(target_arch = "wasm32")]
    pub async fn melt(
        &self,
        proofs: Vec<Proof>,
        invoice: Bolt11Invoice,
        outputs: Option<Vec<BlindedMessage>>,
    ) -> Result<MeltResponse, Error> {
        let url = self.mint_url.join("melt")?;

        let request = MeltRequest {
            proofs,
            pr: invoice,
            outputs,
        };

        let value = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<MeltResponse, serde_json::Error> =
            serde_json::from_value(value.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&value.to_string())?),
        }
    }

    /// Split Token [NUT-06]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn split(&self, split_request: SplitRequest) -> Result<SplitResponse, Error> {
        let url = self.mint_url.join("split")?;

        let res = minreq::post(url)
            .with_json(&split_request)?
            .send()?
            .json::<Value>()?;

        let response: Result<SplitResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) if res.promises.is_some() => Ok(res),
            _ => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Split Token [NUT-06]
    #[cfg(target_arch = "wasm32")]
    pub async fn split(&self, split_request: SplitRequest) -> Result<SplitResponse, Error> {
        let url = self.mint_url.join("split")?;

        let res = Request::post(url.as_str())
            .json(&split_request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<SplitResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Spendable check [NUT-07]
    #[cfg(all(not(target_arch = "wasm32"), feature = "nut07"))]
    pub async fn check_spendable(
        &self,
        proofs: &Vec<nut00::mint::Proof>,
    ) -> Result<CheckSpendableResponse, Error> {
        let url = self.mint_url.join("check")?;
        let request = CheckSpendableRequest {
            proofs: proofs.to_owned(),
        };

        let res = minreq::post(url)
            .with_json(&request)?
            .send()?
            .json::<Value>()?;

        let response: Result<CheckSpendableResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Spendable check [NUT-07]
    #[cfg(all(target_arch = "wasm32", feature = "nut07"))]
    pub async fn check_spendable(
        &self,
        proofs: &Vec<nut00::mint::Proof>,
    ) -> Result<CheckSpendableResponse, Error> {
        let url = self.mint_url.join("check")?;
        let request = CheckSpendableRequest {
            proofs: proofs.to_owned(),
        };

        let res = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<CheckSpendableResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Get Mint Info [NUT-09]
    #[cfg(all(not(target_arch = "wasm32"), feature = "nut09"))]
    pub async fn get_info(&self) -> Result<MintInfo, Error> {
        let url = self.mint_url.join("info")?;
        let res = minreq::get(url).send()?.json::<Value>()?;

        let response: Result<MintInfo, serde_json::Error> = serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Get Mint Info [NUT-09]
    #[cfg(all(target_arch = "wasm32", feature = "nut09"))]
    pub async fn get_info(&self) -> Result<MintInfo, Error> {
        let url = self.mint_url.join("info")?;
        let res = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<MintInfo, serde_json::Error> = serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_decode_error() {
        let err = r#"{"code":0,"error":"Lightning invoice not paid yet."}"#;

        let error = Error::from_json(err).unwrap();

        match error {
            Error::InvoiceNotPaid => {}
            _ => panic!("Wrong error"),
        }

        let err = r#"{"code": 0, "error": "Lightning wallet not responding: Failed to connect to https://legend.lnbits.com due to: All connection attempts failed"}"#;
        let error = Error::from_json(err).unwrap();
        match error {
            Error::LightingWalletNotResponding(mint) => {
                assert_eq!(mint, Some("https://legend.lnbits.com".to_string()));
            }
            _ => panic!("Wrong error"),
        }
    }
}
