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
    contracts: &crate::ActiveContracts,
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
    // todo check consistency

    models::chunked_insert(pool, &nep171_events).await
}
