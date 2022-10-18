use crate::db_adapters::{contracts, Event};
use crate::models;
use crate::models::nft_events::NftEvent;
use futures::future::try_join_all;
use near_lake_framework::near_indexer_primitives;

mod nep171_events;

pub const NFT: &str = "NFT_NEP171";
// pub const NFT_LEGACY: &str = "NFT_LEGACY";

pub(crate) async fn store_nft(
    pool: &sqlx::Pool<sqlx::Postgres>,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    contracts: &contracts::ContractsHelper,
) -> anyhow::Result<()> {
    let mut nep171_events: Vec<NftEvent> = vec![];
    let nft_events_futures = streamer_message.shards.iter().map(|shard| {
        nep171_events::collect_nep171_events(
            &shard.shard_id,
            &shard.receipt_execution_outcomes,
            &streamer_message.block.header,
            contracts,
        )
    });
    for events in try_join_all(nft_events_futures).await? {
        nep171_events.extend(events);
    }
    models::chunked_insert(pool, &nep171_events).await
}

// todo it could be one method both for ft and nft
pub(crate) async fn register_new_contracts(
    nft_events: &mut [NftEvent],
    contracts: &contracts::ContractsHelper,
) -> anyhow::Result<()> {
    let mut new_contracts = std::collections::HashSet::new();
    for event in nft_events.iter().rev() {
        if new_contracts.insert(event.contract_account_id.clone()) {
            contracts
                .try_register_contract(models::contracts::Contract {
                    contract_account_id: event.contract_account_id.clone(),
                    standard: event.standard.clone(),
                    // If the contract was created earlier, this value would be ignored
                    first_event_at_timestamp: event.block_timestamp.clone(),
                    // If the contract was created earlier, this value would be ignored
                    first_event_at_block_height: event.block_height.clone(),
                    inconsistency_found_at_timestamp: None,
                    inconsistency_found_at_block_height: None,
                })
                .await?;
        }
    }
    Ok(())
}

// todo it could be one method both for ft and nft
pub(crate) fn enumerate_events(
    nft_events: &mut [crate::models::nft_events::NftEvent],
    shard_id: &near_indexer_primitives::types::ShardId,
    timestamp: u64,
    event_type: &Event,
) -> anyhow::Result<()> {
    for (index, event) in nft_events.iter_mut().enumerate() {
        event.event_index =
            crate::db_adapters::compose_db_index(timestamp, shard_id, event_type, index)?;
    }
    Ok(())
}
