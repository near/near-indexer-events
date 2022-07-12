mod event_types;
pub(crate) mod events;
pub(crate) mod fungible_token_events;
pub(crate) mod non_fungible_token_events;


pub(crate) const CHUNK_SIZE_FOR_BATCH_INSERT: usize = 100;
