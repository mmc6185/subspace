use futures::future::{Future, Ready};
use sc_client_api::{BlockBackend, HeaderBackend, UsageProvider};
use sc_service::Configuration;
use sc_transaction_pool::{
    error::Result as TxPoolResult, BasicPool, ChainApi, FullChainApi, RevalidationType, Transaction,
};
use sc_transaction_pool_api::{
    ChainEvent, ImportNotificationStream, MaintainedTransactionPool, PoolFuture, PoolStatus,
    ReadyTransactions, TransactionFor, TransactionPool, TransactionStatusStreamFor, TxHash,
};
use sp_api::ProvideRuntimeApi;
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, BlockIdTo, NumberFor},
    transaction_validity::{TransactionSource, TransactionValidity},
};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use substrate_prometheus_endpoint::Registry as PrometheusRegistry;

/// Block hash type for a pool.
pub type BlockHash<A> = <<A as ChainApi>::Block as BlockT>::Hash;

/// Extrinsic hash type for a pool.
pub type ExtrinsicHash<A> = <<A as ChainApi>::Block as BlockT>::Hash;

/// Extrinsic type for a pool.
pub type ExtrinsicFor<A> = <<A as ChainApi>::Block as BlockT>::Extrinsic;

/// A transaction pool for a full node.
pub type FullPool<Block, Client> = BasicPoolWrapper<Block, FullChainApiWrapper<Block, Client>>;

type BoxedReadyIterator<Hash, Data> =
    Box<dyn ReadyTransactions<Item = Arc<Transaction<Hash, Data>>> + Send>;

type ReadyIteratorFor<PoolApi> = BoxedReadyIterator<ExtrinsicHash<PoolApi>, ExtrinsicFor<PoolApi>>;

type PolledIterator<PoolApi> = Pin<Box<dyn Future<Output = ReadyIteratorFor<PoolApi>> + Send>>;

pub struct FullChainApiWrapper<Block, Client> {
    inner: FullChainApi<Client, Block>,
}

impl<Block, Client> FullChainApiWrapper<Block, Client> {
    pub fn new(
        client: Arc<Client>,
        prometheus: Option<&PrometheusRegistry>,
        spawner: &impl SpawnEssentialNamed,
    ) -> Self {
        Self {
            inner: FullChainApi::new(client, prometheus, spawner),
        }
    }
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
    type ValidationFuture = Pin<Box<dyn Future<Output = TxPoolResult<TransactionValidity>> + Send>>;
    type BodyFuture = Ready<TxPoolResult<Option<Vec<<Self::Block as BlockT>::Extrinsic>>>>;

    fn block_body(&self, id: &BlockId<Self::Block>) -> Self::BodyFuture {
        self.inner.block_body(id)
    }

    fn validate_transaction(
        &self,
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        uxt: ExtrinsicFor<Self>,
    ) -> Self::ValidationFuture {
        // TODO: pre-validation

        self.inner.validate_transaction(at, source, uxt)
    }

    fn block_id_to_number(
        &self,
        at: &BlockId<Self::Block>,
    ) -> TxPoolResult<Option<NumberFor<Self::Block>>> {
        self.inner.block_id_to_number(at)
    }

    fn block_id_to_hash(&self, at: &BlockId<Self::Block>) -> TxPoolResult<Option<BlockHash<Self>>> {
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
    PoolApi: ChainApi<Block = Block>,
{
    inner: BasicPool<PoolApi, Block>,
}

impl<Block, PoolApi> BasicPoolWrapper<Block, PoolApi>
where
    Block: BlockT,
    PoolApi: ChainApi<Block = Block> + 'static,
{
    pub fn with_revalidation_type<Client: UsageProvider<Block>>(
        config: &Configuration,
        pool_api: Arc<PoolApi>,
        prometheus: Option<&PrometheusRegistry>,
        spawner: impl SpawnEssentialNamed,
        client: Arc<Client>,
    ) -> Self {
        let basic_pool = BasicPool::with_revalidation_type(
            config.transaction_pool.clone(),
            config.role.is_authority().into(),
            pool_api,
            prometheus,
            RevalidationType::Full,
            spawner,
            client.usage_info().chain.best_number,
        );

        Self { inner: basic_pool }
    }
}

impl<Block, Client> sc_transaction_pool_api::LocalTransactionPool
    for BasicPoolWrapper<Block, FullChainApiWrapper<Block, Client>>
where
    Block: BlockT,
    Client: sp_api::ProvideRuntimeApi<Block>
        + sc_client_api::BlockBackend<Block>
        + sc_client_api::blockchain::HeaderBackend<Block>
        + sp_runtime::traits::BlockIdTo<Block>,
    Client: Send + Sync + 'static,
    Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
    type Block = Block;
    type Hash = ExtrinsicHash<FullChainApiWrapper<Block, Client>>;
    type Error = <FullChainApiWrapper<Block, Client> as ChainApi>::Error;

    fn submit_local(
        &self,
        at: &BlockId<Self::Block>,
        xt: sc_transaction_pool_api::LocalTransactionFor<Self>,
    ) -> Result<Self::Hash, Self::Error> {
        todo!("Impl submit_local")
        /*
        use sp_runtime::{
            traits::SaturatedConversion, transaction_validity::TransactionValidityError,
        };

        let validity = self
            .api
            .validate_transaction_blocking(at, TransactionSource::Local, xt.clone())?
            .map_err(|e| {
                Self::Error::Pool(match e {
                    TransactionValidityError::Invalid(i) => TxPoolError::InvalidTransaction(i),
                    TransactionValidityError::Unknown(u) => TxPoolError::UnknownTransaction(u),
                })
            })?;

        let (hash, bytes) = self.pool.validated_pool().api().hash_and_length(&xt);
        let block_number = self.api.block_id_to_number(at)?.ok_or_else(|| {
            sc_transaction_pool::error::Error::BlockIdConversion(format!("{:?}", at))
        })?;

        let validated = ValidatedTransaction::valid_at(
            block_number.saturated_into::<u64>(),
            hash,
            TransactionSource::Local,
            xt,
            bytes,
            validity,
        );

        self.pool.validated_pool().submit(vec![validated]).remove(0)
        */
    }
}

impl<Block, PoolApi> TransactionPool for BasicPoolWrapper<Block, PoolApi>
where
    Block: BlockT,
    PoolApi: ChainApi<Block = Block> + 'static,
{
    type Block = Block;
    type Hash = ExtrinsicHash<PoolApi>;
    type InPoolTransaction = Transaction<TxHash<Self>, TransactionFor<Self>>;
    type Error = PoolApi::Error;

    fn submit_at(
        &self,
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        xts: Vec<TransactionFor<Self>>,
    ) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
        self.inner.submit_at(at, source, xts)
    }

    fn submit_one(
        &self,
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        xt: TransactionFor<Self>,
    ) -> PoolFuture<TxHash<Self>, Self::Error> {
        self.inner.submit_one(at, source, xt)
    }

    fn submit_and_watch(
        &self,
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        xt: TransactionFor<Self>,
    ) -> PoolFuture<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
        self.inner.submit_and_watch(at, source, xt)
    }

    fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
        self.inner.remove_invalid(hashes)
    }

    fn status(&self) -> PoolStatus {
        self.inner.status()
    }

    fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
        self.inner.import_notification_stream()
    }

    fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
        self.inner.hash_of(xt)
    }

    fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
        self.inner.on_broadcasted(propagations)
    }

    fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
        self.inner.ready_transaction(hash)
    }

    fn ready_at(&self, at: NumberFor<Self::Block>) -> PolledIterator<PoolApi> {
        self.inner.ready_at(at)
    }

    fn ready(&self) -> ReadyIteratorFor<PoolApi> {
        self.inner.ready()
    }
}

impl<Block, PoolApi> MaintainedTransactionPool for BasicPoolWrapper<Block, PoolApi>
where
    Block: BlockT,
    PoolApi: ChainApi<Block = Block> + 'static,
{
    fn maintain(&self, event: ChainEvent<Self::Block>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        self.inner.maintain(event)
    }
}

impl<Block, PoolApi> parity_util_mem::MallocSizeOf for BasicPoolWrapper<Block, PoolApi>
where
    Block: BlockT,
    PoolApi: ChainApi<Block = Block>,
{
    fn size_of(&self, ops: &mut parity_util_mem::MallocSizeOfOps) -> usize {
        self.inner.size_of(ops)
    }
}

pub(super) fn new_full<Block, Client>(
    config: &Configuration,
    spawner: impl SpawnEssentialNamed,
    client: Arc<Client>,
) -> Arc<BasicPoolWrapper<Block, FullChainApiWrapper<Block, Client>>>
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
    let prometheus = config.prometheus_registry();
    let pool_api = Arc::new(FullChainApiWrapper::new(
        client.clone(),
        prometheus,
        &spawner,
    ));
    let pool = Arc::new(BasicPoolWrapper::with_revalidation_type(
        config,
        pool_api,
        prometheus,
        spawner,
        client.clone(),
    ));

    // make transaction pool available for off-chain runtime calls.
    client
        .execution_extensions()
        .register_transaction_pool(&pool);

    pool
}
