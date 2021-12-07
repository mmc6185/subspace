initSidebarItems({"constant":[["CONFIRMATION_DEPTH_K",""],["RECORDED_HISTORY_SEGMENT_SIZE","Recorded History Segment Size includes half of the records (just data records) that will later be erasure coded and together with corresponding witnesses will result in `MERKLE_NUM_LEAVES` pieces of archival history."],["RECORD_SIZE","Size of a segment record given the global piece size (in bytes)."],["REPLICATION_FACTOR","Replication factor, defines minimum desired number of replicas of the blockchain to be stored by the network."]],"mod":[["opaque","Opaque types. These are used by the CLI to instantiate machinery that don’t need to know the specifics of the runtime. They can then be made to be agnostic over specific formats of data like extrinsics, allowing for them to continue syncing the network through upgrades to even the core data structures."]],"type":[["AccountId","Some way of identifying an account on the chain. We intentionally make it equivalent to the public key of our transaction signing scheme."],["Balance","Balance of an account."],["BlockNumber","An index to a block."],["Hash","A hash of some data used by the chain."],["Index","Index of a transaction in the chain."],["Moment","Type used for expressing timestamp."],["Signature","Alias to 512-bit hash when used in the context of a transaction signature on the chain."]]});