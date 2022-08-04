use super::event_types;
use crate::db_adapters::event_types::{Nep141EventKind, Nep171EventKind};
use crate::db_adapters::legacy_ft;
use futures::future::try_join_all;
use near_lake_framework::near_indexer_primitives;

pub(crate) enum Event {
    Nep141,
    Nep171,
    WrapNear,
    RainbowBridge,
    TknNear,
    Wentokensir,
}

// todo it would be nice to expand events standard with the indicator that metadata is updated
pub(crate) async fn store_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<()> {
    // todo we can emit the event twice if the contract started to produce logs. Do something with it
    let events_futures = streamer_message.shards.iter().map(|shard| {
        collect_and_store_events(
            pool,
            json_rpc_client,
            shard,
            &streamer_message.block.header,
            ft_balance_cache,
        )
    });
    try_join_all(events_futures).await.map(|_| ())?;

    let rainbow_bridge_futures = streamer_message.shards.iter().map(|shard| {
        legacy_ft::store_rainbow_bridge(
            pool,
            json_rpc_client,
            &shard.shard_id,
            &shard.receipt_execution_outcomes,
            &streamer_message.block.header,
            ft_balance_cache,
        )
    });
    try_join_all(rainbow_bridge_futures).await.map(|_| ())?;

    let tkn_near_futures = streamer_message.shards.iter().map(|shard| {
        legacy_ft::store_tkn_near(
            pool,
            json_rpc_client,
            &shard.shard_id,
            &shard.receipt_execution_outcomes,
            &streamer_message.block.header,
            ft_balance_cache,
        )
    });
    try_join_all(tkn_near_futures).await.map(|_| ())?;

    let wentokensir_futures = streamer_message.shards.iter().map(|shard| {
        legacy_ft::store_wentokensir(
            pool,
            json_rpc_client,
            &shard.shard_id,
            &shard.receipt_execution_outcomes,
            &streamer_message.block.header,
            ft_balance_cache,
        )
    });
    try_join_all(wentokensir_futures).await.map(|_| ())?;

    let wrap_near_futures = streamer_message.shards.iter().map(|shard| {
        legacy_ft::store_wrap_near(
            pool,
            json_rpc_client,
            &shard.shard_id,
            &shard.receipt_execution_outcomes,
            &streamer_message.block.header,
            ft_balance_cache,
        )
    });
    try_join_all(wrap_near_futures).await.map(|_| ())
}

async fn collect_and_store_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard: &near_indexer_primitives::IndexerShard,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<()> {
    let mut ft_events_with_outcomes = Vec::new();
    let mut nft_events_with_outcomes = Vec::new();
    // FT, NFT, MT can be shuffled. We can have events in blocks like this: FT MT FT NFT FT FT
    // the index should go through each standard
    let mut ft_index: usize = 0;
    let mut nft_index: usize = 0;
    for outcome in &shard.receipt_execution_outcomes {
        let events = extract_events(outcome);
        for event in events {
            match event {
                event_types::NearEvent::Nep141(ft_events) => {
                    let events_count = get_coin_events_count(&ft_events.event_kind);
                    ft_events_with_outcomes.push(super::coin_events::FtEvent {
                        events: ft_events,
                        outcome: outcome.clone(),
                        starting_index: ft_index,
                    });
                    ft_index += events_count;
                }
                event_types::NearEvent::Nep171(nft_events) => {
                    let events_count = get_nft_events_count(&nft_events.event_kind);
                    nft_events_with_outcomes.push(super::nft_events::NftEvent {
                        events: nft_events,
                        outcome: outcome.clone(),
                        starting_index: nft_index,
                    });
                    nft_index += events_count;
                }
            }
        }
    }

    let ft_future = super::coin_events::store_ft_events(
        pool,
        json_rpc_client,
        shard,
        block_header,
        &ft_events_with_outcomes,
        ft_balance_cache,
    );
    let nft_future = super::nft_events::store_nft_events(
        pool,
        shard,
        block_header.timestamp,
        &nft_events_with_outcomes,
    );
    futures::try_join!(ft_future, nft_future)?;
    Ok(())
}

fn get_coin_events_count(event: &Nep141EventKind) -> usize {
    match event {
        Nep141EventKind::FtMint(v) => v.len(),
        // we store 2 lines for each transfer because there are 2 affected account ids
        Nep141EventKind::FtTransfer(v) => v.len() * 2,
        Nep141EventKind::FtBurn(v) => v.len(),
    }
}

fn get_nft_events_count(event: &Nep171EventKind) -> usize {
    match event {
        Nep171EventKind::NftMint(v) => v.len(),
        Nep171EventKind::NftTransfer(v) => v.len(),
        Nep171EventKind::NftBurn(v) => v.len(),
    }
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
