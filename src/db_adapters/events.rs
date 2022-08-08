use super::event_types;
use crate::db_adapters::coin;
use crate::db_adapters::event_types::NearEvent;
use crate::db_adapters::nft::nft_events;
use crate::models;
use crate::models::nft_events::NftEvent;
use futures::future::try_join_all;
use near_lake_framework::near_indexer_primitives;

// todo it would be nice to expand events standard with the indicator that metadata is updated
pub(crate) async fn store_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<()> {
    coin::store_ft(pool, json_rpc_client, streamer_message, ft_balance_cache).await?;

    let mut nep171_events: Vec<NftEvent> = vec![];
    let nft_events_futures = streamer_message.shards.iter().map(|shard| {
        nft_events::store_nft_events(
            &shard.shard_id,
            &shard.receipt_execution_outcomes,
            &streamer_message.block.header,
        )
    });
    for events in try_join_all(nft_events_futures).await? {
        nep171_events.extend(events);
    }

    models::chunked_insert(pool, &nep171_events).await
}

pub(crate) fn extract_events(
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> Vec<NearEvent> {
    let prefix = "EVENT_JSON:";
    outcome.execution_outcome.outcome.logs.iter().filter_map(|untrimmed_log| {
        let log = untrimmed_log.trim();
        if !log.starts_with(prefix) {
            return None;
        }

        match serde_json::from_str::<'_, event_types::NearEvent>(
            log[prefix.len()..].trim(),
        ) {
            Ok(result) => Some(result),
            Err(err) => {
                tracing::warn!(
                    target: crate::INDEXER,
                    "Provided event log does not correspond to any of formats defined in NEP. Will ignore this event. \n {:#?} \n{:#?}",
                    err,
                    untrimmed_log,
                );
                None
            }
        }
    }).collect()
}
