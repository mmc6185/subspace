use sc_service::Configuration;
use sc_transaction_pool::{BasicPool, FullChainApi};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

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
