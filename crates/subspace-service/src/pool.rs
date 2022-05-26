use futures::future::{Future, Ready};
use sc_client_api::{BlockBackend, HeaderBackend};
use sc_service::Configuration;
use sc_transaction_pool::{error::Result as PoolResult, BasicPool, ChainApi, FullChainApi};
use sp_api::ProvideRuntimeApi;
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, BlockIdTo},
    transaction_validity::{TransactionSource, TransactionValidity},
};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::pin::Pin;
use std::sync::Arc;

/// Block hash type for a pool.
pub type BlockHash<A> = <<A as ChainApi>::Block as BlockT>::Hash;

/// Block number type for the ChainApi
pub type NumberFor<A> = sp_runtime::traits::NumberFor<<A as ChainApi>::Block>;

/// Extrinsic hash type for a pool.
pub type ExtrinsicHash<A> = <<A as ChainApi>::Block as BlockT>::Hash;

/// Extrinsic type for a pool.
pub type ExtrinsicFor<A> = <<A as ChainApi>::Block as BlockT>::Extrinsic;

pub struct FullChainApiWrapper<Block, Client> {
    inner: FullChainApi<Client, Block>,
}

impl<Block, Client> ChainApi for FullChainApiWrapper<Block, Client>
where
    Block: BlockT,
    Client: ProvideRuntimeApi<Block>
        + BlockBackend<Block>
        + BlockIdTo<Block>
        + HeaderBackend<Block>
        + Send
        + Sync
        + 'static,
    Client::Api: TaggedTransactionQueue<Block>,
{
    type Block = Block;
    type Error = sc_transaction_pool::error::Error;
    type ValidationFuture = Pin<Box<dyn Future<Output = PoolResult<TransactionValidity>> + Send>>;
    type BodyFuture = Ready<PoolResult<Option<Vec<<Self::Block as BlockT>::Extrinsic>>>>;

    fn block_body(&self, id: &BlockId<Self::Block>) -> Self::BodyFuture {
        self.inner.block_body(id)
    }

    fn validate_transaction(
        &self,
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        uxt: ExtrinsicFor<Self>,
    ) -> Self::ValidationFuture {
        self.inner.validate_transaction(at, source, uxt)
    }

    fn block_id_to_number(&self, at: &BlockId<Self::Block>) -> PoolResult<Option<NumberFor<Self>>> {
        self.inner.block_id_to_number(at)
    }

    fn block_id_to_hash(&self, at: &BlockId<Self::Block>) -> PoolResult<Option<BlockHash<Self>>> {
        self.inner.block_id_to_hash(at)
    }

    fn hash_and_length(&self, ex: &ExtrinsicFor<Self>) -> (ExtrinsicHash<Self>, usize) {
        self.inner.hash_and_length(ex)
    }

    fn block_header(
        &self,
        at: &BlockId<Self::Block>,
    ) -> Result<Option<<Self::Block as BlockT>::Header>, Self::Error> {
        self.inner.block_header(at)
    }
}

pub struct BasicPoolWrapper<Block, PoolApi>
where
    Block: BlockT,
    PoolApi: sc_transaction_pool::ChainApi<Block = Block>,
{
    inner: BasicPool<PoolApi, Block>,
}

pub(super) fn new_full<Block, Client>(
    config: &Configuration,
    spawner: impl SpawnEssentialNamed,
    client: Arc<Client>,
) -> Arc<BasicPool<FullChainApi<Client, Block>, Block>>
where
    Block: BlockT,
    Client: sp_api::ProvideRuntimeApi<Block>
        + sc_client_api::BlockBackend<Block>
        + sc_client_api::blockchain::HeaderBackend<Block>
        + sp_runtime::traits::BlockIdTo<Block>
        + sc_client_api::ExecutorProvider<Block>
        + sc_client_api::UsageProvider<Block>
        + Send
        + Sync
        + 'static,
    Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
    sc_transaction_pool::BasicPool::new_full(
        config.transaction_pool.clone(),
        config.role.is_authority().into(),
        config.prometheus_registry(),
        spawner,
        client,
    )
}
