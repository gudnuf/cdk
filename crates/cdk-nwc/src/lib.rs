//! CDK NWC LN Backend
//!
//! Used for connecting to a Nostr Wallet Connect (NWC) enabled wallet
//! to send and receive payments.
//!
//! The wallet uses NWC notifications to stream payment updates to the mint.

#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![warn(rustdoc::bare_urls)]

use std::cmp::max;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bitcoin::hashes::sha256::Hash;
use cdk_common::amount::{to_unit, Amount};
use cdk_common::common::FeeReserve;
use cdk_common::nuts::{CurrencyUnit, MeltOptions, MeltQuoteState};
use cdk_common::payment::{
    self, Bolt11Settings, CreateIncomingPaymentResponse, IncomingPaymentOptions,
    MakePaymentResponse, MintPayment, OutgoingPaymentOptions, PaymentIdentifier,
    PaymentQuoteResponse, WaitPaymentResponse,
};
use cdk_common::util::hex;
use cdk_common::Bolt11Invoice;
use error::Error;
use futures::stream::StreamExt;
use futures::Stream;
use nwc::prelude::*;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

pub mod error;

/// Connection retry configuration for NWC
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Initial retry delay in seconds
    pub initial_retry_delay: u64,
    /// Maximum retry delay in seconds
    pub max_retry_delay: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Health check interval in seconds
    pub health_check_interval: u64,
    /// Connection timeout for health checks in seconds
    pub connection_timeout: u64,
    /// Timeout for initial validation during startup in seconds
    pub validation_timeout: u64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_retry_delay: 1,
            max_retry_delay: 60,
            backoff_multiplier: 2.0,
            health_check_interval: 30, // Check health every 30 seconds
            connection_timeout: 15,    // 15 second timeout for health checks
            validation_timeout: 30,    // 30 second timeout for initial validation
        }
    }
}

/// NWC Wallet Backend  
#[derive(Clone)]
pub struct NWCWallet {
    /// NWC client
    nwc_client: Arc<NWC>,
    /// Fee reserve configuration
    fee_reserve: FeeReserve,
    /// Channel sender for payment notifications
    #[allow(clippy::type_complexity)]
    sender: tokio::sync::mpsc::Sender<(PaymentIdentifier, Amount, String)>,
    /// Channel receiver for payment notifications
    #[allow(clippy::type_complexity)]
    receiver: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<(PaymentIdentifier, Amount, String)>>>>,
    /// Cancellation token for wait invoice
    wait_invoice_cancel_token: CancellationToken,
    /// Flag indicating if wait invoice is active
    wait_invoice_is_active: Arc<AtomicBool>,
    /// Currency unit
    unit: CurrencyUnit,
    /// Notification handler task handle
    notification_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Connection configuration for retry logic
    connection_config: ConnectionConfig,
    /// Health check cancellation token
    health_check_cancel_token: CancellationToken,
}

impl NWCWallet {
    /// Create new [`NWCWallet`] from NWC URI string with default connection config
    pub async fn new(
        nwc_uri: &str,
        fee_reserve: FeeReserve,
        unit: CurrencyUnit,
    ) -> Result<Self, Error> {
        Self::with_connection_config(nwc_uri, fee_reserve, unit, ConnectionConfig::default()).await
    }

    /// Create new [`NWCWallet`] from NWC URI string with custom connection config
    pub async fn with_connection_config(
        nwc_uri: &str,
        fee_reserve: FeeReserve,
        unit: CurrencyUnit,
        connection_config: ConnectionConfig,
    ) -> Result<Self, Error> {
        // NWC requires TLS for talking to the relay
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }

        let uri = NostrWalletConnectURI::from_str(nwc_uri)
            .map_err(|e| Error::InvalidUri(e.to_string()))?;

        let nwc_client = Arc::new(NWC::new(uri));

        NWCWallet::validate_supported_methods_and_notifications(
            &nwc_client,
            connection_config.validation_timeout,
        )
        .await?;

        let (sender, receiver) = tokio::sync::mpsc::channel(100);

        let wallet = Self {
            nwc_client,
            fee_reserve,
            sender,
            receiver: Arc::new(Mutex::new(Some(receiver))),
            wait_invoice_cancel_token: CancellationToken::new(),
            wait_invoice_is_active: Arc::new(AtomicBool::new(false)),
            unit,
            notification_handle: Arc::new(Mutex::new(None)),
            connection_config,
            health_check_cancel_token: CancellationToken::new(),
        };

        // Start notification handler
        wallet.start_notification_handler().await?;

        // Start health check
        wallet.start_health_check();

        Ok(wallet)
    }

    /// Start the notification handler for payment updates with automatic reconnection
    async fn start_notification_handler(&self) -> Result<(), Error> {
        let nwc_client = self.nwc_client.clone();
        let sender = self.sender.clone();
        let connection_config = self.connection_config.clone();

        let handle = tokio::spawn(async move {
            Self::run_resilient_notification_handler(nwc_client, sender, connection_config).await;
        });

        let mut notification_handle = self.notification_handle.lock().await;
        *notification_handle = Some(handle);

        Ok(())
    }

    /// Run the notification handler with automatic reconnection and exponential backoff
    async fn run_resilient_notification_handler(
        nwc_client: Arc<NWC>,
        sender: tokio::sync::mpsc::Sender<(PaymentIdentifier, Amount, String)>,
        config: ConnectionConfig,
    ) {
        let mut retry_count = 0;
        let mut retry_delay = config.initial_retry_delay;

        loop {
            tracing::info!(
                "NWC: Attempting to establish notification connection (attempt {}/{})",
                retry_count + 1,
                config.max_retries + 1
            );

            match Self::establish_notification_connection(&nwc_client, &sender).await {
                Ok(_) => {
                    tracing::info!("NWC: Notification connection established successfully");
                    // Reset retry count on successful connection
                    retry_count = 0;
                    retry_delay = config.initial_retry_delay;

                    // Connection succeeded, but then failed during operation
                    // This means we should retry the connection
                    tracing::warn!("NWC: Notification connection lost, attempting to reconnect");
                }
                Err(e) => {
                    retry_count += 1;
                    tracing::error!("NWC: Failed to establish notification connection: {}", e);

                    if retry_count > config.max_retries {
                        tracing::error!(
                            "NWC: Exceeded maximum retry attempts ({}), giving up",
                            config.max_retries
                        );
                        return;
                    }

                    tracing::info!(
                        "NWC: Retrying in {} seconds (attempt {}/{})",
                        retry_delay,
                        retry_count + 1,
                        config.max_retries + 1
                    );

                    sleep(Duration::from_secs(retry_delay)).await;

                    // Calculate next retry delay with exponential backoff
                    retry_delay = std::cmp::min(
                        (retry_delay as f64 * config.backoff_multiplier) as u64,
                        config.max_retry_delay,
                    );
                }
            }
        }
    }

    /// Establish a single notification connection and handle notifications until it fails
    async fn establish_notification_connection(
        nwc_client: &Arc<NWC>,
        sender: &tokio::sync::mpsc::Sender<(PaymentIdentifier, Amount, String)>,
    ) -> Result<(), Error> {
        // Subscribe to notifications
        nwc_client
            .subscribe_to_notifications()
            .await
            .map_err(|e| Error::Connection(e.to_string()))?;

        tracing::info!("NWC: Successfully subscribed to notifications");

        // Handle notifications until connection fails
        let result = nwc_client
            .handle_notifications(|notification| {
                let sender = sender.clone();

                async move {
                    match notification.notification_type {
                        NotificationType::PaymentReceived => {
                            if let Ok(payment) = notification.to_pay_notification() {
                                tracing::debug!(
                                    "NWC: Payment received: {:?}",
                                    payment.payment_hash
                                );

                                let payment_hash = match Hash::from_str(&payment.payment_hash) {
                                    Ok(hash) => hash,
                                    Err(e) => {
                                        tracing::error!("NWC: Failed to parse payment hash: {}", e);
                                        return Ok(false);
                                    }
                                };

                                let payment_id =
                                    PaymentIdentifier::PaymentHash(*payment_hash.as_ref());

                                let amount = Amount::from(payment.amount / 1000); // Convert msat to sat

                                // Send notification through channel
                                if let Err(e) = sender
                                    .send((payment_id, amount, payment.payment_hash))
                                    .await
                                {
                                    tracing::error!(
                                        "NWC: Failed to send payment notification: {}",
                                        e
                                    );
                                    return Ok(true); // Exit the notification handler
                                }
                            }
                        }
                        NotificationType::PaymentSent => {
                            // We don't need to handle payment sent notifications
                            // Status can be checked via lookup_invoice when needed
                        }
                    }
                    Ok(false) // Continue processing
                }
            })
            .await;

        match result {
            Ok(_) => {
                tracing::info!("NWC: Notification handler exited normally");
                Ok(())
            }
            Err(e) => {
                tracing::error!("NWC: Notification handler failed: {}", e);
                Err(Error::Connection(e.to_string()))
            }
        }
    }

    /// Start background health check task
    fn start_health_check(&self) {
        let nwc_client = self.nwc_client.clone();
        let config = self.connection_config.clone();
        let cancel_token = self.health_check_cancel_token.clone();

        tokio::spawn(async move {
            Self::run_health_check(nwc_client, config, cancel_token).await;
        });
    }

    /// Run periodic health checks on the NWC connection
    async fn run_health_check(
        nwc_client: Arc<NWC>,
        config: ConnectionConfig,
        cancel_token: CancellationToken,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(config.health_check_interval));

        // Skip the first tick to avoid immediate health check
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match tokio::time::timeout(
                        Duration::from_secs(config.connection_timeout),
                        nwc_client.get_info()
                    ).await {
                        Ok(Ok(_info)) => {
                            tracing::debug!("NWC: Health check passed");
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("NWC: Health check failed: {}", e);
                            // We don't restart the connection here as the notification handler
                            // will detect the failure and restart automatically
                        }
                        Err(_) => {
                            tracing::warn!("NWC: Health check timed out after {} seconds", config.connection_timeout);
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    tracing::info!("NWC: Health check task cancelled");
                    break;
                }
            }
        }
    }

    /// Get connection status information
    pub async fn connection_status(&self) -> Result<String, Error> {
        match tokio::time::timeout(
            Duration::from_secs(self.connection_config.connection_timeout),
            self.nwc_client.get_info(),
        )
        .await
        {
            Ok(Ok(info)) => Ok(format!(
                "Connected - Methods: {:?}, Notifications: {:?}",
                info.methods, info.notifications
            )),
            Ok(Err(e)) => Err(Error::Connection(format!("Health check failed: {}", e))),
            Err(_) => Err(Error::Connection("Health check timed out".to_string())),
        }
    }

    /// Get current connection configuration
    pub fn get_connection_config(&self) -> &ConnectionConfig {
        &self.connection_config
    }

    /// Check if outgoing payment is already paid
    async fn check_outgoing_unpaid(
        &self,
        payment_identifier: &PaymentIdentifier,
    ) -> Result<(), payment::Error> {
        let pay_state = self.check_outgoing_payment(payment_identifier).await?;

        match pay_state.status {
            MeltQuoteState::Unpaid | MeltQuoteState::Unknown | MeltQuoteState::Failed => Ok(()),
            MeltQuoteState::Paid => {
                tracing::debug!("NWC: Melt attempted on invoice already paid");
                Err(payment::Error::InvoiceAlreadyPaid)
            }
            MeltQuoteState::Pending => {
                tracing::debug!("NWC: Melt attempted on invoice already pending");
                Err(payment::Error::InvoicePaymentPending)
            }
        }
    }
}

#[async_trait]
impl MintPayment for NWCWallet {
    type Err = payment::Error;

    #[instrument(skip_all)]
    async fn get_settings(&self) -> Result<Value, Self::Err> {
        Ok(serde_json::to_value(Bolt11Settings {
            mpp: false,
            unit: self.unit.clone(),
            invoice_description: true,
            amountless: true,
            bolt12: false,
        })?)
    }

    #[instrument(skip_all)]
    fn is_wait_invoice_active(&self) -> bool {
        self.wait_invoice_is_active.load(Ordering::SeqCst)
    }

    #[instrument(skip_all)]
    fn cancel_wait_invoice(&self) {
        self.wait_invoice_cancel_token.cancel()
    }

    #[instrument(skip_all)]
    async fn wait_any_incoming_payment(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = WaitPaymentResponse> + Send>>, Self::Err> {
        tracing::info!("NWC: Starting stream for payment notifications");

        self.wait_invoice_is_active.store(true, Ordering::SeqCst);

        let receiver = self
            .receiver
            .lock()
            .await
            .take()
            .ok_or_else(|| payment::Error::Custom("No receiver available".to_string()))?;

        let unit = self.unit.clone();
        let receiver_stream = ReceiverStream::new(receiver);

        Ok(Box::pin(receiver_stream.map(
            move |(request_lookup_id, payment_amount, payment_id)| WaitPaymentResponse {
                payment_identifier: request_lookup_id,
                payment_amount,
                unit: unit.clone(),
                payment_id,
            },
        )))
    }

    #[instrument(skip_all)]
    async fn get_payment_quote(
        &self,
        unit: &CurrencyUnit,
        options: OutgoingPaymentOptions,
    ) -> Result<PaymentQuoteResponse, Self::Err> {
        let (amount_msat, request_lookup_id) = match options {
            OutgoingPaymentOptions::Bolt11(bolt11_options) => {
                let amount_msat: Amount = if let Some(melt_options) = bolt11_options.melt_options {
                    match melt_options {
                        MeltOptions::Amountless { amountless } => {
                            let amount_msat = amountless.amount_msat;

                            if let Some(invoice_amount) =
                                bolt11_options.bolt11.amount_milli_satoshis()
                            {
                                if invoice_amount != u64::from(amount_msat) {
                                    return Err(payment::Error::AmountMismatch);
                                }
                            }
                            amount_msat
                        }
                        MeltOptions::Mpp { mpp } => mpp.amount,
                    }
                } else {
                    bolt11_options
                        .bolt11
                        .amount_milli_satoshis()
                        .ok_or_else(|| Error::UnknownInvoiceAmount)?
                        .into()
                };

                let payment_id =
                    PaymentIdentifier::PaymentHash(*bolt11_options.bolt11.payment_hash().as_ref());
                (amount_msat, Some(payment_id))
            }
            OutgoingPaymentOptions::Bolt12(_) => {
                return Err(payment::Error::UnsupportedUnit);
            }
        };

        // Convert to target unit
        let amount = to_unit(amount_msat, &CurrencyUnit::Msat, unit)?;

        let relative_fee_reserve =
            (self.fee_reserve.percent_fee_reserve * u64::from(amount) as f32) as u64;
        let absolute_fee_reserve: u64 = self.fee_reserve.min_fee_reserve.into();
        let fee = max(relative_fee_reserve, absolute_fee_reserve);

        Ok(PaymentQuoteResponse {
            request_lookup_id,
            amount,
            fee: fee.into(),
            state: MeltQuoteState::Unpaid,
            unit: unit.clone(),
        })
    }

    #[instrument(skip_all)]
    async fn make_payment(
        &self,
        unit: &CurrencyUnit,
        options: OutgoingPaymentOptions,
    ) -> Result<MakePaymentResponse, Self::Err> {
        match options {
            OutgoingPaymentOptions::Bolt11(bolt11_options) => {
                let bolt11 = bolt11_options.bolt11;
                let payment_identifier =
                    PaymentIdentifier::PaymentHash(*bolt11.payment_hash().as_ref());

                self.check_outgoing_unpaid(&payment_identifier).await?;

                // Determine the amount to pay
                let amount_msat: u64 = if let Some(melt_options) = bolt11_options.melt_options {
                    melt_options.amount_msat().into()
                } else {
                    bolt11
                        .amount_milli_satoshis()
                        .ok_or_else(|| Error::UnknownInvoiceAmount)?
                };

                // Create pay invoice request with amount for amountless invoices
                let mut request = PayInvoiceRequest::new(bolt11.to_string());

                // If the invoice is amountless, set the amount
                if bolt11.amount_milli_satoshis().is_none() {
                    request.amount = Some(amount_msat);
                }

                // Make payment through NWC
                let response = self.nwc_client.pay_invoice(request).await.map_err(|e| {
                    tracing::error!("NWC payment failed: {}", e);
                    payment::Error::Lightning(Box::new(e))
                })?;

                let total_spent = to_unit(amount_msat, &CurrencyUnit::Msat, unit)?;
                let fee_paid = if let Some(fees) = response.fees_paid {
                    to_unit(fees, &CurrencyUnit::Msat, unit)?
                } else {
                    Amount::ZERO
                };

                Ok(MakePaymentResponse {
                    payment_proof: Some(response.preimage),
                    payment_lookup_id: payment_identifier,
                    status: MeltQuoteState::Paid,
                    total_spent: total_spent + fee_paid,
                    unit: unit.clone(),
                })
            }
            OutgoingPaymentOptions::Bolt12(_) => Err(payment::Error::UnsupportedUnit),
        }
    }

    #[instrument(skip_all)]
    async fn create_incoming_payment_request(
        &self,
        unit: &CurrencyUnit,
        options: IncomingPaymentOptions,
    ) -> Result<CreateIncomingPaymentResponse, Self::Err> {
        match options {
            IncomingPaymentOptions::Bolt11(bolt11_options) => {
                let description = bolt11_options.description.unwrap_or_default();
                let amount = bolt11_options.amount;
                let expiry = bolt11_options.unix_expiry;

                if amount == Amount::ZERO {
                    return Err(payment::Error::Custom(
                        "NWC requires invoice amount".to_string(),
                    ));
                }

                // Convert amount to millisatoshis
                let amount_msat = to_unit(amount, unit, &CurrencyUnit::Msat)?.into();

                let request_builder = MakeInvoiceRequest {
                    amount: amount_msat,
                    description: if description.is_empty() {
                        None
                    } else {
                        Some(description)
                    },
                    description_hash: None,
                    expiry: None, // Expiry from bolt11_options is too long for NWC
                };

                let response = self
                    .nwc_client
                    .make_invoice(request_builder)
                    .await
                    .map_err(|e| {
                        tracing::error!("NWC create invoice failed: {}", e);
                        payment::Error::Lightning(Box::new(e))
                    })?;

                let payment_hash = *Bolt11Invoice::from_str(&response.invoice)?
                    .payment_hash()
                    .as_ref();

                Ok(CreateIncomingPaymentResponse {
                    request_lookup_id: PaymentIdentifier::PaymentHash(payment_hash),
                    request: response.invoice,
                    expiry,
                })
            }
            IncomingPaymentOptions::Bolt12(_) => Err(payment::Error::UnsupportedUnit),
        }
    }

    #[instrument(skip_all)]
    async fn check_incoming_payment_status(
        &self,
        request_lookup_id: &PaymentIdentifier,
    ) -> Result<Vec<WaitPaymentResponse>, Self::Err> {
        match request_lookup_id {
            PaymentIdentifier::PaymentHash(payment_hash) => {
                let payment_hash_str = hex::encode(payment_hash);

                // Use lookup_invoice to check for this specific payment
                let lookup_request = LookupInvoiceRequest {
                    payment_hash: Some(payment_hash_str),
                    invoice: None,
                };

                match self.nwc_client.lookup_invoice(lookup_request).await {
                    Ok(invoice) => {
                        // Check if this is an incoming payment that has been settled
                        if let Some(TransactionType::Incoming) = invoice.transaction_type {
                            if invoice.settled_at.is_some() {
                                let response = WaitPaymentResponse {
                                    payment_identifier: request_lookup_id.clone(),
                                    payment_amount: Amount::from(invoice.amount / 1000), // Convert msat to sat
                                    unit: self.unit.clone(),
                                    payment_id: invoice.payment_hash,
                                };
                                Ok(vec![response])
                            } else {
                                Ok(vec![]) // Invoice exists but not settled
                            }
                        } else {
                            Ok(vec![]) // Not an incoming payment
                        }
                    }
                    Err(_) => Ok(vec![]), // Invoice not found
                }
            }
            _ => {
                tracing::error!(
                    "NWC: Unsupported payment identifier type for check_incoming_payment_status"
                );
                Err(payment::Error::UnknownPaymentState)
            }
        }
    }

    #[instrument(skip_all)]
    async fn check_outgoing_payment(
        &self,
        request_lookup_id: &PaymentIdentifier,
    ) -> Result<MakePaymentResponse, Self::Err> {
        match request_lookup_id {
            PaymentIdentifier::PaymentHash(payment_hash) => {
                let payment_hash_str = hex::encode(payment_hash);

                // Use lookup_invoice to check the actual payment status
                let lookup_request = LookupInvoiceRequest {
                    payment_hash: Some(payment_hash_str),
                    invoice: None,
                };

                match self.nwc_client.lookup_invoice(lookup_request).await {
                    Ok(invoice) => {
                        if let Some(TransactionType::Outgoing) = invoice.transaction_type {
                            let status =
                                if invoice.settled_at.is_some() || invoice.preimage.is_some() {
                                    MeltQuoteState::Paid
                                } else {
                                    MeltQuoteState::Pending
                                };

                            let total_spent = if status == MeltQuoteState::Paid {
                                to_unit(
                                    invoice.amount + invoice.fees_paid,
                                    &CurrencyUnit::Msat,
                                    &self.unit,
                                )?
                            } else {
                                Amount::ZERO
                            };

                            Ok(MakePaymentResponse {
                                payment_proof: invoice.preimage,
                                payment_lookup_id: request_lookup_id.clone(),
                                status,
                                total_spent,
                                unit: self.unit.clone(),
                            })
                        } else {
                            // Not an outgoing payment
                            Err(payment::Error::UnknownPaymentState)
                        }
                    }
                    Err(e) => {
                        tracing::warn!("NWC: Failed to lookup payment: {}", e);
                        // Return failed status instead of crashing
                        // TODO: melt quotes can get created even if no payment has been attempted yet,
                        // figure a better way to handle this
                        Ok(MakePaymentResponse {
                            payment_proof: None,
                            payment_lookup_id: request_lookup_id.clone(),
                            status: MeltQuoteState::Unknown,
                            total_spent: Amount::ZERO,
                            unit: self.unit.clone(),
                        })
                    }
                }
            }
            _ => {
                tracing::error!(
                    "NWC: Unsupported payment identifier type for check_outgoing_payment"
                );
                Err(payment::Error::UnknownPaymentState)
            }
        }
    }
}

impl NWCWallet {
    async fn validate_supported_methods_and_notifications(
        client: &NWC,
        timeout_secs: u64,
    ) -> Result<(), Error> {
        let info = match tokio::time::timeout(Duration::from_secs(timeout_secs), client.get_info())
            .await
        {
            Ok(result) => result?,
            Err(_) => return Err(Error::Connection("Timeout during validation".to_string())),
        };

        let required_methods = [
            "pay_invoice",
            "get_balance",
            "make_invoice",
            "lookup_invoice",
            "list_transactions",
            "get_info",
        ];

        let missing_methods: Vec<&str> = required_methods
            .iter()
            .filter(|&method| !info.methods.contains(&method.to_string()))
            .copied()
            .collect();

        if !missing_methods.is_empty() {
            return Err(Error::UnsupportedMethods(missing_methods.join(", ")));
        }

        let required_notifications = ["payment_received"];

        let missing_notifications: Vec<&str> = required_notifications
            .iter()
            .filter(|&notification| !info.notifications.contains(&notification.to_string()))
            .copied()
            .collect();

        if !missing_notifications.is_empty() {
            return Err(Error::UnsupportedNotifications(
                missing_notifications.join(", "),
            ));
        }

        Ok(())
    }
}

impl Drop for NWCWallet {
    fn drop(&mut self) {
        tracing::info!("Drop called on NWCWallet");
        self.wait_invoice_cancel_token.cancel();
        self.health_check_cancel_token.cancel();

        // Cancel notification handler task if it exists
        // We need to use blocking approach since Drop is synchronous
        if let Some(handle) = self
            .notification_handle
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take())
        {
            handle.abort();
        }

        // Spawn background task to handle async unsubscription
        let client = self.nwc_client.clone();
        tokio::spawn(async move {
            if let Err(e) = client.unsubscribe_from_notifications().await {
                tracing::warn!(
                    "Failed to unsubscribe from NWC notifications during cleanup: {}",
                    e
                );
            }
        });
    }
}
