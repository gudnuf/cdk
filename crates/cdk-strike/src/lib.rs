//! CDK lightning backend for Strike

#![warn(missing_docs)]
#![warn(rustdoc::bare_urls)]

use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use axum::Router;
use cdk_common::amount::Amount;
use cdk_common::nuts::{CurrencyUnit, MeltOptions, MeltQuoteState, MintQuoteState};
use cdk_common::payment::{
    self, Bolt11Settings, CreateIncomingPaymentResponse, MakePaymentResponse, MintPayment,
    PaymentQuoteResponse,
};
use cdk_common::util::unix_time;
use cdk_common::{mint, Bolt11Invoice};
use error::Error;
use futures::stream::StreamExt;
use futures::Stream;
use serde_json::Value;
use strike_rs::{
    Amount as StrikeAmount, Currency as StrikeCurrencyUnit, InvoiceRequest, InvoiceState,
    PayInvoiceQuoteRequest, Strike as StrikeApi,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub mod error;

/// Strike
#[derive(Clone)]
pub struct Strike {
    strike_api: StrikeApi,
    unit: CurrencyUnit,
    receiver: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<String>>>>,
    webhook_url: String,
    wait_invoice_cancel_token: CancellationToken,
    wait_invoice_is_active: Arc<AtomicBool>,
    pending_invoices: Arc<Mutex<Vec<String>>>,
}

impl Strike {
    /// Create new [`Strike`] wallet
    pub async fn new(
        api_key: String,
        unit: CurrencyUnit,
        receiver: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<String>>>>,
        webhook_url: String,
    ) -> Result<Self, Error> {
        let strike = StrikeApi::new(&api_key, None).map_err(|e| {
            tracing::error!("Failed to create Strike API client: {}", e);
            Error::StrikeRs(e.to_string())
        })?;

        tracing::info!("Successfully created Strike backend");

        Ok(Self {
            strike_api: strike,
            receiver,
            unit,
            webhook_url,
            wait_invoice_cancel_token: CancellationToken::new(),
            wait_invoice_is_active: Arc::new(AtomicBool::new(false)),
            pending_invoices: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

#[async_trait]
impl MintPayment for Strike {
    type Err = payment::Error;

    async fn get_settings(&self) -> Result<Value, Self::Err> {
        let settings = Bolt11Settings {
            mpp: false,
            unit: self.unit.clone(),
            invoice_description: true,
            amountless: false,
        };

        Ok(serde_json::to_value(settings)?)
    }

    fn is_wait_invoice_active(&self) -> bool {
        self.wait_invoice_is_active.load(Ordering::SeqCst)
    }

    fn cancel_wait_invoice(&self) {
        tracing::info!("Cancelling wait invoice for Strike backend");
        self.wait_invoice_cancel_token.cancel()
    }

    #[allow(clippy::incompatible_msrv)]
    async fn wait_any_incoming_payment(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = String> + Send>>, Self::Err> {
        let subscriptions = self.strike_api.get_current_subscriptions().await?;

        // TODO: instead of deleting, the existing subscriptions should be used and the secret should be updated
        // Delete any existing subscriptions
        for subscription in subscriptions {
            self.strike_api
                .delete_subscription(&subscription.id)
                .await
                .map_err(|e| Error::StrikeRs(e.to_string()))?;
        }

        // Create new subscription
        self.strike_api
            .subscribe_to_invoice_webhook(self.webhook_url.clone())
            .await
            .map_err(|e| Error::StrikeRs(e.to_string()))?;

        let receiver = self
            .receiver
            .lock()
            .await
            .take()
            .ok_or(anyhow!("No receiver"))?;

        let strike_api = self.strike_api.clone();
        let cancel_token = self.wait_invoice_cancel_token.clone();

        Ok(futures::stream::unfold(
            (
                receiver,
                strike_api,
                cancel_token,
                Arc::clone(&self.wait_invoice_is_active),
            ),
            |(mut receiver, strike_api, cancel_token, is_active)| async move {
                tokio::select! {

                    _ = cancel_token.cancelled() => {
                        // Stream is cancelled
                        is_active.store(false, Ordering::SeqCst);
                        tracing::info!("Waiting for Strike invoice ending");
                        None
                    }

                    msg_option = receiver.recv() => {
                match msg_option {
                    Some(msg) => {
                        let check = strike_api.get_incoming_invoice(&msg).await;

                        match check {
                            Ok(invoice) => {
                                if invoice.state == InvoiceState::Paid {
                                    Some((msg, (receiver, strike_api, cancel_token, is_active)))
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    }
                    None => None,
                }

                    }
                }
            },
        )
        .boxed())
    }

    async fn get_payment_quote(
        &self,
        request: &str,
        unit: &CurrencyUnit,
        options: Option<MeltOptions>,
    ) -> Result<PaymentQuoteResponse, Self::Err> {
        let bolt11 = Bolt11Invoice::from_str(request)?;

        let amount_msat = match options {
            Some(amount) => amount.amount_msat(),
            None => bolt11
                .amount_milli_satoshis()
                .ok_or(Error::UnknownInvoiceAmount)?
                .into(),
        };

        if unit != &self.unit {
            tracing::warn!(
                "Unsupported unit requested: {:?}, expected: {:?}",
                unit,
                self.unit
            );
            return Err(Self::Err::UnsupportedUnit);
        }

        let source_currency = match unit {
            CurrencyUnit::Sat => StrikeCurrencyUnit::BTC,
            CurrencyUnit::Msat => StrikeCurrencyUnit::BTC,
            CurrencyUnit::Usd => StrikeCurrencyUnit::USD,
            CurrencyUnit::Eur => StrikeCurrencyUnit::EUR,
            _ => {
                tracing::warn!("Unsupported currency unit: {:?}", unit);
                return Err(Self::Err::UnsupportedUnit);
            }
        };

        let payment_quote_request = PayInvoiceQuoteRequest {
            ln_invoice: request.to_string(),
            source_currency,
        };

        let quote = self
            .strike_api
            .payment_quote(payment_quote_request)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get payment quote from Strike: {}", e);
                Error::StrikeRs(e.to_string())
            })?;

        let fee = from_strike_amount(quote.lightning_network_fee, unit)?;
        let amount = from_strike_amount(quote.amount, unit)?;

        let response = PaymentQuoteResponse {
            request_lookup_id: quote.payment_quote_id,
            amount: amount.into(),
            unit: self.unit.clone(),
            fee: fee.into(),
            state: MeltQuoteState::Unpaid,
        };

        Ok(response)
    }

    async fn make_payment(
        &self,
        melt_quote: mint::MeltQuote,
        _partial_amount: Option<Amount>,
        _max_fee: Option<Amount>,
    ) -> Result<MakePaymentResponse, Self::Err> {
        tracing::info!(
            "Making payment with Strike for quote: {}",
            melt_quote.request_lookup_id
        );

        let pay_response = self
            .strike_api
            .pay_quote(&melt_quote.request_lookup_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to pay quote via Strike: {}", e);
                Error::StrikeRs(e.to_string())
            })?;

        let state = match pay_response.state {
            InvoiceState::Paid => {
                tracing::info!("Strike payment completed successfully");
                MeltQuoteState::Paid
            }
            InvoiceState::Unpaid => {
                tracing::warn!("Strike payment failed - unpaid");
                MeltQuoteState::Unpaid
            }
            InvoiceState::Completed => {
                tracing::info!("Strike payment completed");
                MeltQuoteState::Paid
            }
            InvoiceState::Pending => {
                tracing::info!("Strike payment is pending");
                MeltQuoteState::Pending
            }
        };

        let total_spent = from_strike_amount(pay_response.total_amount, &melt_quote.unit)?.into();

        let response = MakePaymentResponse {
            payment_lookup_id: pay_response.payment_id,
            payment_proof: None,
            status: state,
            total_spent,
            unit: melt_quote.unit,
        };

        Ok(response)
    }

    async fn create_incoming_payment_request(
        &self,
        amount: Amount,
        unit: &CurrencyUnit,
        description: String,
        unix_expiry: Option<u64>,
    ) -> Result<CreateIncomingPaymentResponse, Self::Err> {
        tracing::info!("Creating incoming payment request via Strike");

        let time_now = unix_time();
        if let Some(expiry) = unix_expiry {
            assert!(expiry > time_now);
        }
        let request_lookup_id = Uuid::new_v4();

        let strike_amount = to_strike_unit(amount, unit)?;

        let invoice_request = InvoiceRequest {
            correlation_id: Some(request_lookup_id.to_string()),
            amount: strike_amount,
            description: Some(description),
        };

        let create_invoice_response = self
            .strike_api
            .create_invoice(invoice_request)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create invoice via Strike: {}", e);
                Error::StrikeRs(e.to_string())
            })?;

        let quote = self
            .strike_api
            .invoice_quote(&create_invoice_response.invoice_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get invoice quote from Strike: {}", e);
                Error::StrikeRs(e.to_string())
            })?;

        let request: Bolt11Invoice = quote.ln_invoice.parse()?;
        let expiry = request.expires_at().map(|t| t.as_secs());

        // Add the invoice to the pending list for monitoring
        {
            let mut pending_list = self.pending_invoices.lock().await;
            pending_list.push(create_invoice_response.invoice_id.clone());
        }

        let response = CreateIncomingPaymentResponse {
            request_lookup_id: create_invoice_response.invoice_id,
            request: quote.ln_invoice,
            expiry,
        };

        tracing::info!("Successfully created incoming payment request");
        Ok(response)
    }

    async fn check_incoming_payment_status(
        &self,
        request_lookup_id: &str,
    ) -> Result<MintQuoteState, Self::Err> {
        let invoice = self
            .strike_api
            .get_incoming_invoice(request_lookup_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get incoming invoice from Strike: {}", e);
                Error::StrikeRs(e.to_string())
            })?;

        let state = match invoice.state {
            InvoiceState::Paid => {
                tracing::info!("Incoming payment {} is paid", request_lookup_id);
                MintQuoteState::Paid
            }
            InvoiceState::Unpaid => MintQuoteState::Unpaid,
            InvoiceState::Completed => {
                tracing::info!("Incoming payment {} is completed", request_lookup_id);
                MintQuoteState::Paid
            }
            InvoiceState::Pending => MintQuoteState::Pending,
        };

        Ok(state)
    }

    async fn check_outgoing_payment(
        &self,
        payment_id: &str,
    ) -> Result<MakePaymentResponse, Self::Err> {
        let invoice = self.strike_api.get_outgoing_payment(payment_id).await;

        let pay_invoice_response = match invoice {
            Ok(invoice) => {
                let state = match invoice.state {
                    InvoiceState::Paid => {
                        tracing::info!("Outgoing payment {} is paid", payment_id);
                        MeltQuoteState::Paid
                    }
                    InvoiceState::Unpaid => {
                        tracing::warn!("Outgoing payment {} is unpaid", payment_id);
                        MeltQuoteState::Unpaid
                    }
                    InvoiceState::Completed => {
                        tracing::info!("Outgoing payment {} is completed", payment_id);
                        MeltQuoteState::Paid
                    }
                    InvoiceState::Pending => MeltQuoteState::Pending,
                };

                let total_spent = from_strike_amount(invoice.total_amount, &self.unit)?.into();

                MakePaymentResponse {
                    payment_lookup_id: invoice.payment_id,
                    payment_proof: None,
                    status: state,
                    total_spent,
                    unit: self.unit.clone(),
                }
            }
            Err(err) => match err {
                strike_rs::Error::NotFound => {
                    tracing::warn!("Outgoing payment not found: {}", payment_id);
                    MakePaymentResponse {
                        payment_lookup_id: payment_id.to_string(),
                        payment_proof: None,
                        status: MeltQuoteState::Unknown,
                        total_spent: Amount::ZERO,
                        unit: self.unit.clone(),
                    }
                }
                _ => {
                    tracing::error!("Error checking outgoing payment: {}", err);
                    return Err(Error::StrikeRs(err.to_string()).into());
                }
            },
        };

        Ok(pay_invoice_response)
    }
}

impl Strike {
    /// Create invoice webhook router
    pub async fn create_invoice_webhook(
        &self,
        webhook_endpoint: &str,
        sender: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<Router> {
        self.strike_api
            .create_invoice_webhook_router(webhook_endpoint, sender)
            .await
    }
}

pub(crate) fn from_strike_amount(
    strike_amount: StrikeAmount,
    target_unit: &CurrencyUnit,
) -> anyhow::Result<u64> {
    match target_unit {
        CurrencyUnit::Sat => strike_amount.to_sats(),
        CurrencyUnit::Msat => Ok(strike_amount.to_sats()? * 1000),
        CurrencyUnit::Usd => {
            if strike_amount.currency == StrikeCurrencyUnit::USD {
                Ok((strike_amount.amount * 100.0).round() as u64)
            } else {
                bail!("Could not convert strike USD");
            }
        }
        CurrencyUnit::Eur => {
            if strike_amount.currency == StrikeCurrencyUnit::EUR {
                Ok((strike_amount.amount * 100.0).round() as u64)
            } else {
                bail!("Could not convert to EUR");
            }
        }
        _ => bail!("Unsupported unit"),
    }
}

pub(crate) fn to_strike_unit<T>(
    amount: T,
    current_unit: &CurrencyUnit,
) -> anyhow::Result<StrikeAmount>
where
    T: Into<u64>,
{
    let amount = amount.into();
    match current_unit {
        CurrencyUnit::Sat => Ok(StrikeAmount::from_sats(amount)),
        CurrencyUnit::Msat => Ok(StrikeAmount::from_sats(amount / 1000)),
        CurrencyUnit::Usd => {
            let dollars = (amount as f64 / 100_f64) * 100.0;

            Ok(StrikeAmount {
                currency: StrikeCurrencyUnit::USD,
                amount: dollars.round() / 100.0,
            })
        }
        CurrencyUnit::Eur => {
            let euro = (amount as f64 / 100_f64) * 100.0;

            Ok(StrikeAmount {
                currency: StrikeCurrencyUnit::EUR,
                amount: euro.round() / 100.0,
            })
        }
        _ => bail!("Unsupported unit"),
    }
}
