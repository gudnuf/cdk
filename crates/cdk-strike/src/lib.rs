//! CDK lightning backend for Strike

#![warn(missing_docs)]
#![warn(rustdoc::bare_urls)]

use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
    Amount as StrikeAmount, Currency as StrikeCurrencyUnit, CurrencyExchangeQuoteRequest,
    ExchangeAmount, ExchangeQuoteState, FeePolicy, InvoiceQueryParams, InvoiceRequest,
    InvoiceState, PayInvoiceQuoteRequest, Strike as StrikeApi,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub mod error;

/// Strike
#[derive(Clone)]
pub struct Strike {
    // API client and configuration
    strike_api: StrikeApi,
    unit: CurrencyUnit,
    webhook_url: String,
    // Invoice state and communication
    receiver: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<String>>>>,
    wait_invoice_cancel_token: CancellationToken,
    wait_invoice_is_active: Arc<AtomicBool>,
    pending_invoices: Arc<Mutex<HashMap<String, u64>>>, // invoice_id -> creation_time // NOTE: these were added for polling
}

impl std::fmt::Debug for Strike {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Strike")
            .field("unit", &self.unit)
            .field("webhook_url", &self.webhook_url)
            .field(
                "wait_invoice_is_active",
                &self.wait_invoice_is_active.load(Ordering::SeqCst),
            )
            .field(
                "pending_invoices_count",
                &self
                    .pending_invoices
                    .try_lock()
                    .map(|m| m.len())
                    .unwrap_or(0),
            )
            .finish()
    }
}

impl Strike {
    /// Create new [`Strike`] wallet
    pub async fn new(
        api_key: String,
        unit: CurrencyUnit,
        receiver: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<String>>>>,
        webhook_url: String,
    ) -> Result<Self, Error> {
        let strike = StrikeApi::new(&api_key, None).map_err(Error::from)?;

        tracing::info!("Successfully created Strike backend");

        Ok(Self {
            strike_api: strike,
            receiver,
            unit,
            webhook_url,
            wait_invoice_cancel_token: CancellationToken::new(),
            wait_invoice_is_active: Arc::new(AtomicBool::new(false)),
            pending_invoices: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Lookup an invoice by correlation id. Returns the first invoice if found, or an error if not found.
    async fn lookup_invoice_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<strike_rs::InvoiceListItem, Error> {
        let query_params = InvoiceQueryParams::new()
            .filter(strike_rs::Filter::eq("correlationId", correlation_id));
        let invoice_list = self
            .strike_api
            .get_invoices(Some(query_params))
            .await
            .map_err(Error::from)?;
        let invoice = invoice_list.items.first().cloned();
        match invoice {
            Some(inv) => Ok(inv),
            None => {
                tracing::error!("No invoice found for correlation id: {}", correlation_id);
                Err(Error::Anyhow(anyhow!(
                    "No invoice found for correlation id: {}",
                    correlation_id
                )))
            }
        }
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
        // let subscriptions = self.strike_api.get_current_subscriptions().await.map_err(Error::from)?;

        // // TODO: instead of deleting, the existing subscriptions should be used and the secret should be updated
        // // Delete any existing subscriptions
        // for subscription in subscriptions {
        //     self.strike_api
        //         .delete_subscription(&subscription.id)
        //         .await
        //         .map_err(|e| Error::StrikeRs(e.to_string()))?;
        // }

        let receiver = self
            .receiver
            .lock()
            .await
            .take()
            .ok_or(anyhow!("No receiver"))?;

        let strike_api = self.strike_api.clone();
        let cancel_token = self.wait_invoice_cancel_token.clone();
        let pending_invoices = Arc::clone(&self.pending_invoices);
        let is_active = Arc::clone(&self.wait_invoice_is_active);

        // Try to create new subscription, but if it fails, just log and continue with polling
        match self
            .strike_api
            .subscribe_to_invoice_webhook(self.webhook_url.clone())
            .await
        {
            Ok(_) => {
                tracing::debug!("Created new subscription for webhook: {}", self.webhook_url);
                // Only use the receiver stream, no polling
                let stream = futures::stream::unfold(
                    (receiver, cancel_token, is_active),
                    |(mut receiver, cancel_token, is_active)| async move {
                        tokio::select! {
                            _ = cancel_token.cancelled() => {
                                is_active.store(false, Ordering::SeqCst);
                                tracing::info!("Waiting for Strike invoice ending (webhook only mode)");
                                None
                            }
                            msg_option = receiver.recv() => {
                                match msg_option {
                                    Some(msg) => Some((msg, (receiver, cancel_token, is_active))),
                                    None => None,
                                }
                            }
                        }
                    },
                )
                .filter_map(|item| async move {
                    if item.is_empty() {
                        None
                    } else {
                        Some(item)
                    }
                })
                .boxed();
                Ok(stream)
            }
            Err(e) => {
                tracing::warn!("Failed to create Strike webhook subscription (falling back to polling only): {}", e);
                // Fallback to polling stream as before
                Ok(futures::stream::unfold(
                    (
                        receiver,
                        strike_api,
                        cancel_token,
                        is_active,
                        pending_invoices,
                        tokio::time::Instant::now(),
                    ),
                    |(mut receiver, strike_api, cancel_token, is_active, pending_invoices, mut last_poll)| async move {
                        // Set up a 10-second polling interval
                        let poll_interval = Duration::from_secs(10);
                        let mut poll_timer = tokio::time::interval_at(last_poll + poll_interval, poll_interval);
                        poll_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
                                                    // Remove from pending invoices if it was there
                                                    {
                                                        let mut pending = pending_invoices.lock().await;
                                                        pending.remove(&msg);
                                                    }
                                                    Some((msg, (receiver, strike_api, cancel_token, is_active, pending_invoices, last_poll)))
                                                } else {
                                                    Some((String::new(), (receiver, strike_api, cancel_token, is_active, pending_invoices, last_poll)))
                                                }
                                            }
                                            _ => Some((String::new(), (receiver, strike_api, cancel_token, is_active, pending_invoices, last_poll)))
                                        }
                                    }
                                    None => Some((String::new(), (receiver, strike_api, cancel_token, is_active, pending_invoices, last_poll)))
                                }
                            }

                            _ = poll_timer.tick() => {
                                last_poll = tokio::time::Instant::now();

                                // Poll all pending invoices
                                let mut invoices_to_check = Vec::new();
                                {
                                    let pending = pending_invoices.lock().await;
                                    for (invoice_id, _creation_time) in pending.iter() {
                                        invoices_to_check.push(invoice_id.clone());
                                    }
                                }

                                for invoice_id in invoices_to_check {
                                    match strike_api.get_incoming_invoice(&invoice_id).await {
                                        Ok(invoice) => {
                                            if invoice.state == InvoiceState::Paid {
                                                tracing::info!("Polling detected paid invoice: {}", invoice_id);
                                                // Remove from pending invoices
                                                {
                                                    let mut pending = pending_invoices.lock().await;
                                                    pending.remove(&invoice_id);
                                                }
                                                return Some((invoice_id, (receiver, strike_api, cancel_token, is_active, pending_invoices, last_poll)));
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("Error polling invoice {}: {}", invoice_id, e);
                                            // Remove errored invoices from pending list to avoid repeated errors
                                            {
                                                let mut pending = pending_invoices.lock().await;
                                                pending.remove(&invoice_id);
                                            }
                                        }
                                    }
                                }

                                // Clean up old invoices (older than 24 hours)
                                let current_time = unix_time();
                                let twenty_four_hours = 24 * 60 * 60;
                                {
                                    let mut pending = pending_invoices.lock().await;
                                    pending.retain(|_invoice_id, creation_time| {
                                        current_time - *creation_time < twenty_four_hours
                                    });
                                }

                                Some((String::new(), (receiver, strike_api, cancel_token, is_active, pending_invoices, last_poll)))
                            }
                        }
                    },
                )
                .filter_map(|item| async move {
                    if item.is_empty() {
                        None
                    } else {
                        Some(item)
                    }
                })
                .boxed())
            }
        }
    }

    async fn get_payment_quote(
        &self,
        request: &str,
        unit: &CurrencyUnit,
        _: Option<MeltOptions>,
    ) -> Result<PaymentQuoteResponse, Self::Err> {
        let bolt11 = Bolt11Invoice::from_str(request)?;
        let description = bolt11.description().to_string();

        let correlation_id = description
            .split("TXID:")
            .nth(1)
            .and_then(|txid_part| txid_part.split_whitespace().next())
            .filter(|txid| !txid.is_empty());

        match correlation_id {
            Some(correlation_id) => {
                tracing::debug!(
                    "Found correlation ID in payment request: {:?}",
                    correlation_id
                );
            }
            None => (),
        }

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

        let internal_invoice = if let Some(correlation_id) = correlation_id {
            Some(
                self.lookup_invoice_by_correlation_id(correlation_id)
                    .await?,
            )
        } else {
            None
        };

        if let Some(internal_invoice) = internal_invoice {
            tracing::info!("Internal invoice found, processing internal payment");

            if internal_invoice.amount.currency == source_currency {
                tracing::debug!("Internal invoice currency matches source currency");
                let amount = Strike::from_strike_amount(internal_invoice.amount.clone(), unit)?;
                return Ok(PaymentQuoteResponse {
                    request_lookup_id: format!("internal:{}", correlation_id.unwrap()),
                    amount: amount.into(),
                    unit: self.unit.clone(),
                    fee: Amount::ZERO,
                    state: MeltQuoteState::Unpaid,
                });
            } else {
                // Create currency exchange quote, but do not execute
                tracing::debug!(
                    "Internal invoice currency ({:?}) does not match source currency ({:?}). Creating currency exchange quote.",
                    internal_invoice.amount.currency,
                    source_currency
                );
                let currency_to_sell = source_currency;
                let currency_to_buy = internal_invoice.amount.currency.clone();
                let amount_to_buy = internal_invoice.amount.clone();

                let exchange_request = CurrencyExchangeQuoteRequest {
                    sell: currency_to_sell.clone(),
                    buy: currency_to_buy.clone(),
                    amount: ExchangeAmount {
                        amount: amount_to_buy.amount.to_string(),
                        currency: amount_to_buy.currency,
                        fee_policy: Some(FeePolicy::Exclusive),
                    },
                };

                let quote = self
                    .strike_api
                    .create_currency_exchange_quote(exchange_request)
                    .await
                    .map_err(Error::from)?;

                tracing::debug!(
                    "Created currency exchange quote details - ID: {}, Created: {}, Valid until: {}, Source: {} {}, Target: {} {}, Fee: {:?}, Rate: {} {} per {}, State: {:?}",
                    quote.id,
                    quote.created,
                    quote.valid_until,
                    quote.source.amount,
                    quote.source.currency,
                    quote.target.amount,
                    quote.target.currency,
                    quote.fee.as_ref().map(|f| format!("{} {}", f.amount, f.currency)),
                    quote.conversion_rate.amount,
                    quote.conversion_rate.target_currency,
                    quote.conversion_rate.source_currency,
                    quote.state
                );

                let converted_amount =
                    Strike::from_strike_amount(quote.source.clone(), &self.unit)?;

                let fee = if let Some(fee_info) = quote.fee.clone() {
                    if Strike::currency_unit_eq_strike(&self.unit, &fee_info.currency) {
                        Strike::from_strike_amount(fee_info.clone(), &self.unit)?
                    } else {
                        // Convert fee to self.unit using the quote's conversion rate
                        Strike::convert_fee_to_unit(fee_info, &self.unit, quote.conversion_rate)?
                    }
                } else {
                    0
                };

                return Ok(PaymentQuoteResponse {
                    request_lookup_id: format!("exchange:{}", quote.id),
                    amount: converted_amount.into(),
                    unit: self.unit.clone(),
                    fee: fee.into(),
                    state: MeltQuoteState::Unpaid,
                });
            }
        }

        let payment_quote_request = PayInvoiceQuoteRequest {
            ln_invoice: request.to_string(),
            source_currency,
        };

        let quote = self
            .strike_api
            .payment_quote(payment_quote_request)
            .await
            .map_err(Error::from)?;
        let fee = if let Some(fee) = quote.lightning_network_fee {
            Strike::from_strike_amount(fee, unit)?
        } else {
            tracing::warn!(
                "No lightning network fee found for quote {}",
                quote.payment_quote_id
            );
            0
        };

        let amount = Strike::from_strike_amount(quote.amount, unit)?;

        let response = PaymentQuoteResponse {
            request_lookup_id: format!("payment:{}", quote.payment_quote_id),
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

        // Parse label and id from request_lookup_id
        let (label, id) = match melt_quote.request_lookup_id.split_once(":") {
            Some((label, id)) => (label, id),
            None => ("payment", melt_quote.request_lookup_id.as_str()), // fallback for legacy
        };

        match label {
            "internal" => {
                // Internal, same currency
                let internal_invoice = self
                    .lookup_invoice_by_correlation_id(id)
                    .await
                    .map_err(Error::from)?;
                let total_spent =
                    Strike::from_strike_amount(internal_invoice.amount.clone(), &melt_quote.unit)?
                        .into();
                return Ok(MakePaymentResponse {
                    payment_lookup_id: melt_quote.request_lookup_id.clone(),
                    payment_proof: None,
                    status: MeltQuoteState::Paid,
                    total_spent,
                    unit: melt_quote.unit,
                });
            }
            "exchange" => {
                // Currency exchange
                let (converted_amount, _fee) = self.execute_currency_exchange_by_id(id).await?;
                return Ok(MakePaymentResponse {
                    payment_lookup_id: melt_quote.request_lookup_id.clone(),
                    payment_proof: None,
                    status: MeltQuoteState::Paid, // or Pending if async
                    total_spent: converted_amount.into(),
                    unit: melt_quote.unit,
                });
            }
            "payment" | _ => {
                // Regular payment
                let pay_response = self.strike_api.pay_quote(id).await.map_err(Error::from)?;

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
                    InvoiceState::Failed => {
                        tracing::error!("Strike payment failed");
                        MeltQuoteState::Failed
                    }
                };

                let total_spent =
                    Strike::from_strike_amount(pay_response.total_amount, &melt_quote.unit)?.into();

                let response = MakePaymentResponse {
                    payment_lookup_id: pay_response.payment_id,
                    payment_proof: None,
                    status: state,
                    total_spent,
                    unit: melt_quote.unit,
                };

                Ok(response)
            }
        }
    }

    async fn create_incoming_payment_request(
        &self,
        amount: Amount,
        unit: &CurrencyUnit,
        description: String,
        unix_expiry: Option<u64>,
    ) -> Result<CreateIncomingPaymentResponse, Self::Err> {
        let time_now = unix_time();
        if let Some(expiry) = unix_expiry {
            assert!(expiry > time_now);
        }
        let correlation_id = Uuid::new_v4();

        let strike_amount = Strike::to_strike_unit(amount, unit)?;

        let invoice_request = InvoiceRequest {
            correlation_id: Some(correlation_id.to_string()),
            amount: strike_amount,
            description: Some(format!("{} TXID:{}", description, correlation_id)),
        };

        let create_invoice_response = self
            .strike_api
            .create_invoice(invoice_request)
            .await
            .map_err(Error::from)?;

        let quote = self
            .strike_api
            .invoice_quote(&create_invoice_response.invoice_id)
            .await
            .map_err(Error::from)?;

        let request: Bolt11Invoice = quote.ln_invoice.parse()?;
        let expiry = request.expires_at().map(|t| t.as_secs());

        let response = CreateIncomingPaymentResponse {
            request_lookup_id: create_invoice_response.invoice_id.clone(),
            request: quote.ln_invoice,
            expiry,
        };

        // Store the invoice ID for polling
        {
            let mut pending_invoices = self.pending_invoices.lock().await;
            pending_invoices.insert(create_invoice_response.invoice_id, time_now);
        }

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
            .map_err(Error::from)?;

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
            InvoiceState::Failed => {
                tracing::error!("Incoming payment {} is failed", request_lookup_id);
                MintQuoteState::Unpaid
            }
        };

        Ok(state)
    }

    async fn check_outgoing_payment(
        &self,
        payment_lookup_id: &str,
    ) -> Result<MakePaymentResponse, Self::Err> {
        tracing::info!(
            "Checking outgoing payment with lookup id: {}",
            payment_lookup_id
        );

        // Parse label and id from payment_lookup_id
        let (label, id) = match payment_lookup_id.split_once(":") {
            Some((label, id)) => (label, id),
            None => ("payment", payment_lookup_id), // fallback for legacy
        };

        match label {
            "internal" => {
                // Internal payment - check invoice by correlation ID
                let internal_invoice = self
                    .lookup_invoice_by_correlation_id(id)
                    .await
                    .map_err(Error::from)?;
                let state = match internal_invoice.state {
                    InvoiceState::Paid => {
                        tracing::info!("Internal payment {} is paid", id);
                        MeltQuoteState::Paid
                    }
                    InvoiceState::Unpaid => {
                        tracing::warn!("Internal payment {} is unpaid", id);
                        MeltQuoteState::Unpaid
                    }
                    InvoiceState::Completed => {
                        tracing::info!("Internal payment {} is completed", id);
                        MeltQuoteState::Paid
                    }
                    InvoiceState::Pending => MeltQuoteState::Pending,
                    InvoiceState::Failed => {
                        tracing::error!("Internal payment {} is failed", id);
                        MeltQuoteState::Failed
                    }
                };

                let total_spent =
                    Strike::from_strike_amount(internal_invoice.amount.clone(), &self.unit)?.into();

                Ok(MakePaymentResponse {
                    payment_lookup_id: payment_lookup_id.to_string(),
                    payment_proof: None,
                    status: state,
                    total_spent,
                    unit: self.unit.clone(),
                })
            }
            "exchange" => {
                // Currency exchange - check exchange quote status
                let quote = self
                    .strike_api
                    .get_currency_exchange_quote(id)
                    .await
                    .map_err(Error::from)?;

                let state = match quote.state {
                    ExchangeQuoteState::Completed => MeltQuoteState::Paid,
                    ExchangeQuoteState::Failed => MeltQuoteState::Failed,
                    ExchangeQuoteState::New => MeltQuoteState::Unpaid,
                    ExchangeQuoteState::Pending => MeltQuoteState::Pending,
                };

                let total_spent =
                    Strike::from_strike_amount(quote.source.clone(), &self.unit)?.into();

                Ok(MakePaymentResponse {
                    payment_lookup_id: payment_lookup_id.to_string(),
                    payment_proof: None,
                    status: state,
                    total_spent,
                    unit: self.unit.clone(),
                })
            }
            "payment" | _ => {
                // Regular payment - check via Strike API
                let invoice = self.strike_api.get_outgoing_payment(id).await;

                let pay_invoice_response = match invoice {
                    Ok(invoice) => {
                        let state = match invoice.state {
                            InvoiceState::Paid => {
                                tracing::info!("Outgoing payment {} is paid", id);
                                MeltQuoteState::Paid
                            }
                            InvoiceState::Unpaid => {
                                tracing::warn!("Outgoing payment {} is unpaid", id);
                                MeltQuoteState::Unpaid
                            }
                            InvoiceState::Completed => {
                                tracing::info!("Outgoing payment {} is completed", id);
                                MeltQuoteState::Paid
                            }
                            InvoiceState::Pending => MeltQuoteState::Pending,
                            InvoiceState::Failed => {
                                tracing::error!("Outgoing payment {} is failed", id);
                                MeltQuoteState::Failed
                            }
                        };

                        let total_spent =
                            Strike::from_strike_amount(invoice.total_amount, &self.unit)?.into();

                        MakePaymentResponse {
                            payment_lookup_id: payment_lookup_id.to_string(),
                            payment_proof: None,
                            status: state,
                            total_spent,
                            unit: self.unit.clone(),
                        }
                    }
                    Err(err) => match err {
                        strike_rs::Error::NotFound => {
                            tracing::warn!("Outgoing payment not found: {}", id);
                            MakePaymentResponse {
                                payment_lookup_id: payment_lookup_id.to_string(),
                                payment_proof: None,
                                status: MeltQuoteState::Unknown,
                                total_spent: Amount::ZERO,
                                unit: self.unit.clone(),
                            }
                        }
                        _ => {
                            tracing::error!("Error checking outgoing payment: {}", err);
                            return Err(Error::from(err).into());
                        }
                    },
                };

                Ok(pay_invoice_response)
            }
        }
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

    /// Execute currency exchange for internal payment (by quote id only)
    async fn execute_currency_exchange_by_id(&self, quote_id: &str) -> Result<(u64, u64), Error> {
        match self
            .strike_api
            .execute_currency_exchange_quote(quote_id)
            .await
        {
            Ok(_) => (),
            Err(strike_rs::Error::ApiError(api_error)) => {
                if api_error
                    .is_error_code(&strike_rs::StrikeErrorCode::CurrencyExchangeQuoteExpired)
                {
                    tracing::warn!("Currency exchange quote {} has expired", quote_id);
                    return Err(Error::Anyhow(anyhow!(
                        "Currency exchange quote has expired"
                    )));
                } else {
                    return Err(strike_rs::Error::ApiError(api_error).into());
                }
            }
            Err(e) => return Err(e.into()),
        }
        // After execution, fetch the quote to get the amounts/fees
        let quote = self
            .strike_api
            .get_currency_exchange_quote(quote_id)
            .await
            .map_err(Error::from)?;
        let converted_amount = Strike::from_strike_amount(quote.source.clone(), &self.unit)?;
        let fee = if let Some(fee_info) = quote.fee.clone() {
            if Strike::currency_unit_eq_strike(&self.unit, &fee_info.currency) {
                Strike::from_strike_amount(fee_info.clone(), &self.unit)?
            } else {
                Strike::convert_fee_to_unit(fee_info, &self.unit, quote.conversion_rate)?
            }
        } else {
            0
        };
        Ok((converted_amount, fee))
    }
}

// Group helper functions into a trait for clarity
trait StrikeHelpers {
    fn from_strike_amount(
        strike_amount: StrikeAmount,
        target_unit: &CurrencyUnit,
    ) -> anyhow::Result<u64>;
    fn to_strike_unit<T: Into<u64>>(
        amount: T,
        current_unit: &CurrencyUnit,
    ) -> anyhow::Result<StrikeAmount>;
    fn currency_unit_eq_strike(unit: &CurrencyUnit, strike: &StrikeCurrencyUnit) -> bool;
    fn convert_fee_to_unit(
        fee_amount: StrikeAmount,
        target_unit: &CurrencyUnit,
        rate: strike_rs::ConversionRate,
    ) -> anyhow::Result<u64>;
}

impl StrikeHelpers for Strike {
    fn from_strike_amount(
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

    fn to_strike_unit<T: Into<u64>>(
        amount: T,
        current_unit: &CurrencyUnit,
    ) -> anyhow::Result<StrikeAmount> {
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

    fn currency_unit_eq_strike(unit: &CurrencyUnit, strike: &StrikeCurrencyUnit) -> bool {
        match (unit, strike) {
            (CurrencyUnit::Sat, StrikeCurrencyUnit::BTC) => true,
            (CurrencyUnit::Msat, StrikeCurrencyUnit::BTC) => true, // msat is subunit of BTC
            (CurrencyUnit::Usd, StrikeCurrencyUnit::USD) => true,
            (CurrencyUnit::Eur, StrikeCurrencyUnit::EUR) => true,
            _ => false,
        }
    }

    fn convert_fee_to_unit(
        fee_amount: StrikeAmount,
        target_unit: &CurrencyUnit,
        rate: strike_rs::ConversionRate,
    ) -> anyhow::Result<u64> {
        // Only support conversion between BTC (sats) and USD/EUR for now
        let rate = rate.amount;
        match (&fee_amount.currency, target_unit) {
            (StrikeCurrencyUnit::USD, CurrencyUnit::Sat)
            | (StrikeCurrencyUnit::EUR, CurrencyUnit::Sat) => {
                // rate: X USD per BTC, so 1 USD = 1/X BTC = 100_000_000/X sats
                let sats = (fee_amount.amount * 100_000_000.0 / rate).round() as u64;
                Ok(sats)
            }
            (StrikeCurrencyUnit::USD, CurrencyUnit::Msat)
            | (StrikeCurrencyUnit::EUR, CurrencyUnit::Msat) => {
                let msats = (fee_amount.amount * 100_000_000_000.0 / rate).round() as u64;
                Ok(msats)
            }
            (StrikeCurrencyUnit::USD, CurrencyUnit::Usd)
            | (StrikeCurrencyUnit::EUR, CurrencyUnit::Eur) => {
                // fee is already in correct fiat unit, return as cents
                Ok((fee_amount.amount * 100.0).round() as u64)
            }
            _ => Err(anyhow!(
                "Unsupported fee currency/unit conversion: {:?} -> {:?}",
                fee_amount.currency,
                target_unit
            )),
        }
    }
}
