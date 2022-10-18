use near_jsonrpc_primitives::types::query::{QueryResponseKind, RpcQueryError};

pub(crate) fn get_function_call_request(
    block_hash: &near_primitives::hash::CryptoHash,
    account_id: near_primitives::types::AccountId,
    method_name: &str,
    args: serde_json::Value,
) -> near_jsonrpc_client::methods::query::RpcQueryRequest {
    near_jsonrpc_client::methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::BlockReference::BlockId(
            near_primitives::types::BlockId::Hash(*block_hash),
        ),
        request: near_primitives::views::QueryRequest::CallFunction {
            account_id,
            method_name: method_name.to_string(),
            args: near_primitives::types::FunctionArgs::from(args.to_string().into_bytes()),
        },
    }
}

pub(crate) async fn wrapped_call(
    rpc_client: &near_jsonrpc_client::JsonRpcClient,
    request: near_jsonrpc_client::methods::query::RpcQueryRequest,
    block_hash: &near_primitives::hash::CryptoHash,
    contract_id: &near_primitives::types::AccountId,
) -> anyhow::Result<near_primitives::views::CallResult> {
    tracing::info!(
        target: crate::LOGGING_PREFIX,
        "RPC request: {:?}\nTo contract:{}, block {}",
        request,
        contract_id,
        block_hash
    );
    match rpc_client.call(request).await {
        Ok(response) => match response.kind {
            QueryResponseKind::CallResult(result) => Ok(result),
            _ => anyhow::bail!("Unexpected type of the response after CallFunction request"),
        },
        Err(x) => {
            if let Some(RpcQueryError::ContractExecutionError { vm_error, .. }) = x.handler_error()
            {
                if vm_error.contains("CodeDoesNotExist") || vm_error.contains("MethodNotFound") {
                    anyhow::bail!(
                        "The account `{}` does not implement any suitable contract at block {}",
                        contract_id,
                        block_hash
                    )
                }
            }
            Err(x.into())
        }
    }
}
