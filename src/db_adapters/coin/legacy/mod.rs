use crate::models::coin_events::CoinEvent;
use futures::try_join;
use near_lake_framework::near_indexer_primitives;

mod aurora;
mod rainbow_bridge;
mod skyward;
mod tkn_near;
mod wentokensir;
mod wrap_near;

pub(crate) async fn collect_legacy(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    ft_balance_cache: &crate::FtBalanceCache,
    contracts: &crate::ActiveContracts,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut events: Vec<CoinEvent> = vec![];

    let aurora_future = aurora::collect_aurora(
        json_rpc_client,
        shard_id,
        receipt_execution_outcomes,
        block_header,
        ft_balance_cache,
        contracts,
    );
    let rainbow_bridge_future = rainbow_bridge::collect_rainbow_bridge(
        json_rpc_client,
        shard_id,
        receipt_execution_outcomes,
        block_header,
        ft_balance_cache,
        contracts,
    );
    let skyward_future = skyward::collect_skyward(
        json_rpc_client,
        shard_id,
        receipt_execution_outcomes,
        block_header,
        ft_balance_cache,
        contracts,
    );
    let tkn_near_future = tkn_near::collect_tkn_near(
        json_rpc_client,
        shard_id,
        receipt_execution_outcomes,
        block_header,
        ft_balance_cache,
        contracts,
    );
    let wentokensir_future = wentokensir::collect_wentokensir(
        json_rpc_client,
        shard_id,
        receipt_execution_outcomes,
        block_header,
        ft_balance_cache,
        contracts,
    );
    let wrap_near_future = wrap_near::collect_wrap_near(
        json_rpc_client,
        shard_id,
        receipt_execution_outcomes,
        block_header,
        ft_balance_cache,
        contracts,
    );

    let (
        aurora_events,
        rainbow_bridge_events,
        skyward_events,
        tkn_near_events,
        wentokensir_events,
        wrap_near_events,
    ) = try_join!(
        aurora_future,
        rainbow_bridge_future,
        skyward_future,
        tkn_near_future,
        wentokensir_future,
        wrap_near_future
    )?;

    events.extend(aurora_events);
    events.extend(rainbow_bridge_events);
    events.extend(skyward_events);
    events.extend(tkn_near_events);
    events.extend(wentokensir_events);
    events.extend(wrap_near_events);
    Ok(events)
}
