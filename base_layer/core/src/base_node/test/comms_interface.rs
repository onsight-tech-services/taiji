// Copyright 2019. The Tari Project
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

use crate::{
    base_node::comms_interface::{
        CommsInterfaceError,
        InboundNodeCommsInterface,
        NodeCommsRequest,
        NodeCommsRequestType,
        NodeCommsResponse,
        OutboundNodeCommsInterface,
    },
    blocks::{genesis_block::get_genesis_block, BlockHeader},
    chain_storage::{BlockchainDatabase, ChainMetadata, DbTransaction, HistoricalBlock, MemoryDatabase},
    proof_of_work::Difficulty,
    tari_amount::MicroTari,
    test_utils::builders::{add_block_and_update_header, create_test_kernel, create_utxo},
    types::HashDigest,
};
use futures::{executor::block_on, StreamExt};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_test_utils::runtime::test_async;
use tari_utilities::hash::Hashable;

async fn test_request_responder(
    receiver: &mut Receiver<
        (NodeCommsRequest, NodeCommsRequestType),
        Result<Vec<NodeCommsResponse>, CommsInterfaceError>,
    >,
    response: Vec<NodeCommsResponse>,
)
{
    let req_context = receiver.next().await.unwrap();
    req_context.reply(Ok(response)).unwrap()
}

#[test]
fn outbound_get_metadata() {
    let (sender, mut receiver) = reply_channel::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(sender);

    block_on(async {
        let metadata1 = ChainMetadata::new(5, vec![0u8], 2.into(), 3);
        let metadata2 = ChainMetadata::new(6, vec![1u8], 3.into(), 4);
        let metadata_response: Vec<NodeCommsResponse> = vec![
            NodeCommsResponse::ChainMetadata(metadata1.clone()),
            NodeCommsResponse::ChainMetadata(metadata2.clone()),
        ];
        let (received_metadata, _) = futures::join!(
            outbound_nci.get_metadata(),
            test_request_responder(&mut receiver, metadata_response)
        );
        let received_metadata = received_metadata.unwrap();
        assert_eq!(received_metadata.len(), 2);
        assert!(received_metadata.contains(&metadata1));
        assert!(received_metadata.contains(&metadata2));
    });
}

#[test]
fn inbound_get_metadata() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let inbound_nci = InboundNodeCommsInterface::new(store);

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::ChainMetadata(received_metadata)) =
                inbound_nci.handle_request(&NodeCommsRequest::GetChainMetadata).await
            {
                assert_eq!(received_metadata.height_of_longest_chain, None);
                assert_eq!(received_metadata.best_block, None);
                assert_eq!(received_metadata.total_accumulated_difficulty, Difficulty::from(0));
                assert_eq!(received_metadata.pruning_horizon, 0);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_kernels() {
    let (sender, mut receiver) = reply_channel::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(sender);

    block_on(async {
        let kernel = create_test_kernel(5.into(), 0);
        let hash = kernel.hash();
        let kernel_response: Vec<NodeCommsResponse> = vec![NodeCommsResponse::TransactionKernels(vec![kernel.clone()])];
        let (received_kernels, _) = futures::join!(
            outbound_nci.fetch_kernels(vec![hash]),
            test_request_responder(&mut receiver, kernel_response)
        );
        let received_kernels = received_kernels.unwrap();
        assert_eq!(received_kernels.len(), 1);
        assert_eq!(received_kernels[0], kernel);
    });
}

#[test]
fn inbound_fetch_kernels() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let inbound_nci = InboundNodeCommsInterface::new(store.clone());

    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone());
    assert!(store.commit(txn).is_ok());

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::TransactionKernels(received_kernels)) = inbound_nci
                .handle_request(&NodeCommsRequest::FetchKernels(vec![hash]))
                .await
            {
                assert_eq!(received_kernels.len(), 1);
                assert_eq!(received_kernels[0], kernel);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_headers() {
    let (sender, mut receiver) = reply_channel::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(sender);

    block_on(async {
        let mut header = BlockHeader::new(0);
        header.height = 0;
        let header_response: Vec<NodeCommsResponse> = vec![NodeCommsResponse::BlockHeaders(vec![header.clone()])];
        let (received_headers, _) = futures::join!(
            outbound_nci.fetch_headers(vec![0]),
            test_request_responder(&mut receiver, header_response)
        );
        let received_headers = received_headers.unwrap();
        assert_eq!(received_headers.len(), 1);
        assert_eq!(received_headers[0], header);
    });
}

#[test]
fn inbound_fetch_headers() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let inbound_nci = InboundNodeCommsInterface::new(store.clone());

    let mut header = BlockHeader::new(0);
    header.height = 0;
    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone());
    assert!(store.commit(txn).is_ok());

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::BlockHeaders(received_headers)) = inbound_nci
                .handle_request(&NodeCommsRequest::FetchHeaders(vec![0]))
                .await
            {
                assert_eq!(received_headers.len(), 1);
                assert_eq!(received_headers[0], header);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_utxos() {
    let (sender, mut receiver) = reply_channel::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(sender);

    block_on(async {
        let (utxo, _) = create_utxo(MicroTari(10_000));
        let hash = utxo.hash();
        let utxo_response: Vec<NodeCommsResponse> = vec![NodeCommsResponse::TransactionOutputs(vec![utxo.clone()])];
        let (received_utxos, _) = futures::join!(
            outbound_nci.fetch_utxos(vec![hash]),
            test_request_responder(&mut receiver, utxo_response)
        );
        let received_utxos = received_utxos.unwrap();
        assert_eq!(received_utxos.len(), 1);
        assert_eq!(received_utxos[0], utxo);
    });
}

#[test]
fn inbound_fetch_utxos() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let inbound_nci = InboundNodeCommsInterface::new(store.clone());

    let (utxo, _) = create_utxo(MicroTari(10_000));
    let hash = utxo.hash();
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone());
    assert!(store.commit(txn).is_ok());

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::TransactionOutputs(received_utxos)) = inbound_nci
                .handle_request(&NodeCommsRequest::FetchUtxos(vec![hash]))
                .await
            {
                assert_eq!(received_utxos.len(), 1);
                assert_eq!(received_utxos[0], utxo);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_blocks() {
    let (sender, mut receiver) = reply_channel::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(sender);

    block_on(async {
        let block = HistoricalBlock::new(get_genesis_block(), 0, Vec::new());
        let block_response: Vec<NodeCommsResponse> = vec![NodeCommsResponse::HistoricalBlocks(vec![block.clone()])];
        let (received_blocks, _) = futures::join!(
            outbound_nci.fetch_blocks(vec![0]),
            test_request_responder(&mut receiver, block_response)
        );
        let received_blocks = received_blocks.unwrap();
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0], block);
    });
}

#[test]
fn inbound_fetch_blocks() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let inbound_nci = InboundNodeCommsInterface::new(store.clone());

    let block = add_block_and_update_header(&store, get_genesis_block());

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nci
                .handle_request(&NodeCommsRequest::FetchBlocks(vec![0]))
                .await
            {
                assert_eq!(received_blocks.len(), 1);
                assert_eq!(*received_blocks[0].block(), block);
            } else {
                assert!(false);
            }
        });
    });
}
