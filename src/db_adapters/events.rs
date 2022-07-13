use super::event_types;
use near_lake_framework::near_indexer_primitives;
use tracing::warn;

pub(crate) async fn store_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    streamer_message: &near_indexer_primitives::StreamerMessage,
) -> anyhow::Result<()> {
    let futures = streamer_message.shards.iter().map(|shard| {
        collect_and_store_events(pool, shard, streamer_message.block.header.timestamp)
    });

    futures::future::try_join_all(futures).await.map(|_| ())
}

async fn collect_and_store_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shard: &near_indexer_primitives::IndexerShard,
    block_timestamp: u64,
) -> anyhow::Result<()> {
    let mut ft_events_with_outcomes = Vec::new();
    let mut nft_events_with_outcomes = Vec::new();

    for outcome in &shard.receipt_execution_outcomes {
        let events = extract_events(outcome);
        for event in events {
            match event {
                event_types::NearEvent::Nep141(ft_event) => {
                    ft_events_with_outcomes.push((ft_event, outcome));
                }
                event_types::NearEvent::Nep171(nft_event) => {
                    nft_events_with_outcomes.push((nft_event, outcome));
                }
            }
        }
    }

    let ft_future = super::fungible_token_events::store_ft_events(
        pool,
        shard,
        block_timestamp,
        &ft_events_with_outcomes,
    );
    let nft_future = super::non_fungible_token_events::store_nft_events(
        pool,
        shard,
        block_timestamp,
        &nft_events_with_outcomes,
    );
    futures::try_join!(ft_future, nft_future)?;
    Ok(())
}

fn extract_events(
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> Vec<event_types::NearEvent> {
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
                warn!(
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
