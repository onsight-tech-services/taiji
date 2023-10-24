// Copyright 2023, OnSight Tech Services LLC
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{marker::PhantomData, sync::Arc};

use futures::{Stream, StreamExt};
use log::*;
use taiji_comms_dht::Dht;
use taiji_core::{
    consensus::ConsensusManager,
    proto::base_node as base_node_proto,
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        transaction_protocol::proto::protocol as proto,
        CryptoFactories,
    },
};
use taiji_p2p::{
    comms_connector::SubscriptionFactory,
    domain_message::DomainMessage,
    services::utils::map_decode,
    taiji_message::TaijiMessageType,
};
use taiji_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::broadcast;

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
    connectivity_service::WalletConnectivityHandle,
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionServiceHandle,
        service::TransactionService,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
    util::wallet_identity::WalletIdentity,
};

pub mod config;
pub mod error;
pub mod handle;
pub mod protocols;
pub mod service;
pub mod storage;
pub mod tasks;
mod utc;

const LOG_TARGET: &str = "wallet::transaction_service";
const SUBSCRIPTION_LABEL: &str = "Transaction Service";

pub struct TransactionServiceInitializer<T, W, TKeyManagerInterface>
where
    T: TransactionBackend,
    W: WalletBackend,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    config: TransactionServiceConfig,
    subscription_factory: Arc<SubscriptionFactory>,
    tx_backend: Option<T>,
    wallet_identity: WalletIdentity,
    consensus_manager: ConsensusManager,
    factories: CryptoFactories,
    wallet_database: Option<WalletDatabase<W>>,
    _phantom_data: PhantomData<TKeyManagerInterface>,
}

impl<T, W, TKeyManagerInterface> TransactionServiceInitializer<T, W, TKeyManagerInterface>
where
    T: TransactionBackend,
    W: WalletBackend,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub fn new(
        config: TransactionServiceConfig,
        subscription_factory: Arc<SubscriptionFactory>,
        backend: T,
        wallet_identity: WalletIdentity,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
        wallet_database: WalletDatabase<W>,
    ) -> Self {
        Self {
            config,
            subscription_factory,
            tx_backend: Some(backend),
            wallet_identity,
            consensus_manager,
            factories,
            wallet_database: Some(wallet_database),
            _phantom_data: Default::default(),
        }
    }

    /// Get a stream of inbound Text messages
    fn transaction_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<proto::TransactionSenderMessage, prost::DecodeError>>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TaijiMessageType::SenderPartialTransaction
        );
        self.subscription_factory
            .get_subscription(TaijiMessageType::SenderPartialTransaction, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::TransactionSenderMessage>)
    }

    fn transaction_reply_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<proto::RecipientSignedMessage, prost::DecodeError>>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TaijiMessageType::ReceiverPartialTransactionReply
        );
        self.subscription_factory
            .get_subscription(TaijiMessageType::ReceiverPartialTransactionReply, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::RecipientSignedMessage>)
    }

    fn transaction_finalized_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<proto::TransactionFinalizedMessage, prost::DecodeError>>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TaijiMessageType::TransactionFinalized
        );
        self.subscription_factory
            .get_subscription(TaijiMessageType::TransactionFinalized, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::TransactionFinalizedMessage>)
    }

    fn base_node_response_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<base_node_proto::BaseNodeServiceResponse, prost::DecodeError>>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TaijiMessageType::BaseNodeResponse
        );
        self.subscription_factory
            .get_subscription(TaijiMessageType::BaseNodeResponse, SUBSCRIPTION_LABEL)
            .map(map_decode::<base_node_proto::BaseNodeServiceResponse>)
    }

    fn transaction_cancelled_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<proto::TransactionCancelledMessage, prost::DecodeError>>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TaijiMessageType::TransactionCancelled
        );
        self.subscription_factory
            .get_subscription(TaijiMessageType::TransactionCancelled, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::TransactionCancelledMessage>)
    }
}

#[async_trait]
impl<T, W, TKeyManagerInterface> ServiceInitializer for TransactionServiceInitializer<T, W, TKeyManagerInterface>
where
    T: TransactionBackend + 'static,
    W: WalletBackend + 'static,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        let (sender, receiver) = reply_channel::unbounded();
        let transaction_stream = self.transaction_stream();
        let transaction_reply_stream = self.transaction_reply_stream();
        let transaction_finalized_stream = self.transaction_finalized_stream();
        let base_node_response_stream = self.base_node_response_stream();
        let transaction_cancelled_stream = self.transaction_cancelled_stream();

        let (publisher, _) = broadcast::channel(self.config.transaction_event_channel_size);

        let transaction_handle = TransactionServiceHandle::new(sender, publisher.clone());

        // Register handle before waiting for handles to be ready
        context.register_handle(transaction_handle);

        let tx_backend = self
            .tx_backend
            .take()
            .expect("Cannot start Transaction Service without providing a backend");

        let wallet_database = self
            .wallet_database
            .take()
            .expect("Cannot start Transaction Service without providing a wallet database");

        let wallet_identity = self.wallet_identity.clone();
        let consensus_manager = self.consensus_manager.clone();
        let factories = self.factories.clone();
        let config = self.config.clone();

        context.spawn_when_ready(move |handles| async move {
            let outbound_message_service = handles.expect_handle::<Dht>().outbound_requester();
            let output_manager_service = handles.expect_handle::<OutputManagerHandle>();
            let core_key_manager_service = handles.expect_handle::<TKeyManagerInterface>();
            let connectivity = handles.expect_handle::<WalletConnectivityHandle>();
            let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();

            let result = TransactionService::new(
                config,
                TransactionDatabase::new(tx_backend),
                wallet_database,
                receiver,
                transaction_stream,
                transaction_reply_stream,
                transaction_finalized_stream,
                base_node_response_stream,
                transaction_cancelled_stream,
                output_manager_service,
                core_key_manager_service,
                outbound_message_service,
                connectivity,
                publisher,
                wallet_identity,
                consensus_manager,
                factories,
                handles.get_shutdown_signal(),
                base_node_service_handle,
            )
            .start()
            .await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Transaction Service error: {}", e);
            }
            info!(target: LOG_TARGET, "Transaction Service shutdown");
        });

        Ok(())
    }
}
