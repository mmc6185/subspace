use futures::future::{Future, FutureExt, Ready};
use sc_client_api::{BlockBackend, ExecutorProvider, HeaderBackend, UsageProvider};
use sc_service::Configuration;
use sc_transaction_pool::error::Result as TxPoolResult;
use sc_transaction_pool::{BasicPool, ChainApi, FullChainApi, Pool, RevalidationType, Transaction};
use sc_transaction_pool_api::{
    ChainEvent, ImportNotificationStream, MaintainedTransactionPool, PoolFuture, PoolStatus,
    ReadyTransactions, TransactionFor, TransactionPool, TransactionStatusStreamFor, TxHash,
};
use sp_api::ProvideRuntimeApi;
use sp_core::traits::SpawnEssentialNamed;
use sp_executor::ExecutorApi;
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{Block as BlockT, BlockIdTo, NumberFor};
use sp_runtime::transaction_validity::{TransactionSource, TransactionValidity};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use subspace_fraud_proof::VerifyFraudProof;
use substrate_prometheus_endpoint::Registry as PrometheusRegistry;

/// Block hash type for a pool.
pub type BlockHash<A> = <<A as ChainApi>::Block as BlockT>::Hash;

/// Extrinsic hash type for a pool.
pub type ExtrinsicHash<A> = <<A as ChainApi>::Block as BlockT>::Hash;

/// Extrinsic type for a pool.
pub type ExtrinsicFor<A> = <<A as ChainApi>::Block as BlockT>::Extrinsic;

/// A transaction pool for a full node.
pub type FullPool<Block, PoolClient, Client, Verifier> =
    BasicPoolWrapper<Block, FullChainApiWrapper<Block, PoolClient, Client, Verifier>>;

type BoxedReadyIterator<Hash, Data> =
    Box<dyn ReadyTransactions<Item = Arc<Transaction<Hash, Data>>> + Send>;

type ReadyIteratorFor<PoolApi> = BoxedReadyIterator<ExtrinsicHash<PoolApi>, ExtrinsicFor<PoolApi>>;

type PolledIterator<PoolApi> = Pin<Box<dyn Future<Output = ReadyIteratorFor<PoolApi>> + Send>>;

pub struct FullChainApiWrapper<Block, PoolClient, Client, Verifier> {
    inner: FullChainApi<PoolClient, Block>,
    client: Arc<Client>,
    verifier: Verifier,
}

impl<Block, PoolClient, Client, Verifier> FullChainApiWrapper<Block, PoolClient, Client, Verifier> {
    pub fn new(
        pool_client: Arc<PoolClient>,
        client: Arc<Client>,
        prometheus: Option<&PrometheusRegistry>,
        spawner: &impl SpawnEssentialNamed,
        verifier: Verifier,
    ) -> Self {
        Self {
            inner: FullChainApi::new(pool_client, prometheus, spawner),
            client,
            verifier,
        }
    }
}

impl<Block, PoolClient, Client, Verifier> ChainApi
    for FullChainApiWrapper<Block, PoolClient, Client, Verifier>
where
    Block: BlockT,
    PoolClient: ProvideRuntimeApi<Block>
        + BlockBackend<Block>
        + BlockIdTo<Block>
        + HeaderBackend<Block>
        + Send
        + Sync
        + 'static,
    PoolClient::Api: TaggedTransactionQueue<Block>,
    Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
    Client::Api: ExecutorApi<Block, cirrus_primitives::Hash>,
    Verifier: VerifyFraudProof + Send + Sync + 'static,
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
        let maybe_fraud_proof = self
            .client
            .runtime_api()
            .extract_fraud_proof(at, &uxt)
            .unwrap();

        if let Some(fraud_proof) = maybe_fraud_proof {
            // TODO: pre-validation
            println!("========== fraud_proof: {:?}", fraud_proof);
            if let Err(err) = self.verifier.verify_fraud_proof(&fraud_proof) {
                tracing::debug!(target: "txpool", error = ?err, "Fraud proof verification failure");
                return async move {
                    Err(sc_transaction_pool_api::error::Error::InvalidTransaction(
                        pallet_executor::InvalidTransactionCode::FraudProof.into(),
                    )
                    .into())
                }
                .boxed();
            }
        }

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
    pub fn with_revalidation_type<PoolClient: UsageProvider<Block>>(
        config: &Configuration,
        pool_api: Arc<PoolApi>,
        prometheus: Option<&PrometheusRegistry>,
        spawner: impl SpawnEssentialNamed,
        client: Arc<PoolClient>,
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

    /// Gets shared reference to the underlying pool.
    pub fn pool(&self) -> &Arc<Pool<PoolApi>> {
        self.inner.pool()
    }
}

impl<Block, PoolClient, Client, Verifier> sc_transaction_pool_api::LocalTransactionPool
    for BasicPoolWrapper<Block, FullChainApiWrapper<Block, PoolClient, Client, Verifier>>
where
    Block: BlockT,
    PoolClient: ProvideRuntimeApi<Block>
        + BlockBackend<Block>
        + HeaderBackend<Block>
        + BlockIdTo<Block>
        + Send
        + Sync
        + 'static,
    PoolClient::Api: TaggedTransactionQueue<Block>,
    Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
    Client::Api: ExecutorApi<Block, cirrus_primitives::Hash>,
    Verifier: VerifyFraudProof + Send + Sync + 'static,
{
    type Block = Block;
    type Hash = ExtrinsicHash<FullChainApiWrapper<Block, PoolClient, Client, Verifier>>;
    type Error = <FullChainApiWrapper<Block, PoolClient, Client, Verifier> as ChainApi>::Error;

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

pub(super) fn new_full<Block, PoolClient, Client, Verifier>(
    config: &Configuration,
    spawner: impl SpawnEssentialNamed,
    pool_client: Arc<PoolClient>,
    client: Arc<Client>,
    verifier: Verifier,
) -> Arc<BasicPoolWrapper<Block, FullChainApiWrapper<Block, PoolClient, Client, Verifier>>>
where
    Block: BlockT,
    PoolClient: ProvideRuntimeApi<Block>
        + BlockBackend<Block>
        + HeaderBackend<Block>
        + BlockIdTo<Block>
        + ExecutorProvider<Block>
        + UsageProvider<Block>
        + Send
        + Sync
        + 'static,
    PoolClient::Api: TaggedTransactionQueue<Block>,
    Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
    Client::Api: ExecutorApi<Block, cirrus_primitives::Hash>,
    Verifier: VerifyFraudProof + Send + Sync + 'static,
{
    let prometheus = config.prometheus_registry();
    let pool_api = Arc::new(FullChainApiWrapper::new(
        pool_client.clone(),
        client,
        prometheus,
        &spawner,
        verifier,
    ));
    let pool = Arc::new(BasicPoolWrapper::with_revalidation_type(
        config,
        pool_api,
        prometheus,
        spawner,
        pool_client.clone(),
    ));

    // make transaction pool available for off-chain runtime calls.
    pool_client
        .execution_extensions()
        .register_transaction_pool(&pool);

    pool
}
