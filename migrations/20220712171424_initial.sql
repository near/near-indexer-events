CREATE TABLE non_fungible_token_events
(
    emitted_for_receipt_id                text           NOT NULL,

    -- Next three columns (emitted_at_block_timestamp, emitted_in_shard_id, emitted_index_of_event_entry_in_shard)
    -- should be used for sorting purposes, at the order that we just named.
    emitted_at_block_timestamp            numeric(20, 0) NOT NULL,
    emitted_in_shard_id                   numeric(20, 0) NOT NULL,
    -- `emitted_index_of_event_entry_in_shard` has non-trivial implementation. It combines the order from:
    -- 1. execution_outcomes::index_in_chunk
    -- 2. Index of current action_receipt
    -- 3. Index of event entry that we are currently working on. Note, one receipt can have multiple events
    --    (read: log with multiple statements), each of them can have multiple account_ids and token_ids.
    --    We use continuous numbering for all these items.
    emitted_index_of_event_entry_in_shard integer        NOT NULL,

    -- account_id of the contract itself. In a simple words, it's the owner/creator of NFT contract
    emitted_by_contract_account_id        text           NOT NULL,
    -- Unique ID of the token
    token_id                              text           NOT NULL,
    event_kind                            text           NOT NULL,

    -- We use `NOT NULL DEFAULT ''` in all the lines below to simplify further issue with nulls + constraints
    -- Previous owner of the token. Empty if we have nft_event_kind 'MINT'.
    token_old_owner_account_id            text           NOT NULL DEFAULT '',
    -- New owner of the token. Empty if we have nft_event_kind 'BURN'.
    token_new_owner_account_id            text           NOT NULL DEFAULT '',
    -- The account that initialized the event.
    -- It differs from token_old_owner_account_id, but it is approved to manipulate with current token.
    -- More information here https://nomicon.io/Standards/NonFungibleToken/ApprovalManagement.html
    -- Optional field: filled only if the event is done NOT by token_old_owner_account_id.
    -- Empty if we have nft_event_kind 'MINT'.
    token_authorized_account_id           text           NOT NULL DEFAULT '',
    -- Optional message associated with token movement.
    event_memo                            text           NOT NULL DEFAULT ''
);

ALTER TABLE ONLY non_fungible_token_events
    ADD CONSTRAINT non_fungible_token_events_pkey PRIMARY KEY (
    emitted_at_block_timestamp,
    emitted_in_shard_id,
    emitted_index_of_event_entry_in_shard);

CREATE INDEX non_fungible_token_events_sorting_idx ON non_fungible_token_events
    USING btree (emitted_at_block_timestamp,
    emitted_in_shard_id,
    emitted_index_of_event_entry_in_shard);

CREATE INDEX non_fungible_token_events_block_timestamp_idx ON non_fungible_token_events
    USING btree (emitted_at_block_timestamp);

CREATE INDEX non_fungible_token_events_old_owner_account_id_idx ON non_fungible_token_events
    USING btree (token_old_owner_account_id);

CREATE INDEX non_fungible_token_events_new_owner_account_id_idx ON non_fungible_token_events
    USING btree (token_new_owner_account_id);

ALTER TABLE ONLY non_fungible_token_events
    ADD CONSTRAINT non_fungible_token_events_fk
    FOREIGN KEY (emitted_for_receipt_id) REFERENCES receipts (receipt_id) ON DELETE CASCADE;

CREATE TABLE fungible_token_events
(
    emitted_for_receipt_id                text           NOT NULL,

    -- Next three columns (emitted_at_block_timestamp, emitted_in_shard_id, emitted_index_of_event_entry_in_shard)
    -- should be used for sorting purposes, at the order that we just named.
    emitted_at_block_timestamp            numeric(20, 0) NOT NULL,
    emitted_in_shard_id                   numeric(20, 0) NOT NULL,
    -- `emitted_index_of_event_entry_in_shard` has non-trivial implementation. It combines the order from:
    -- 1. execution_outcomes::index_in_chunk
    -- 2. Index of current action_receipt
    -- 3. Index of event entry that we are currently working on. Note, one receipt can have multiple events
    --    (read: log with multiple statements), each of them can have multiple account_ids and token_ids.
    --    We use continuous numbering for all these items.
    emitted_index_of_event_entry_in_shard integer        NOT NULL,

    -- account_id of the contract itself. In a simple words, it's the owner/creator of FT contract
    emitted_by_contract_account_id        text           NOT NULL,
    amount                                text           NOT NULL,
    event_kind                            text           NOT NULL,

    -- We use `NOT NULL DEFAULT ''` in all the lines below to simplify further issue with nulls + constraints
    -- Previous owner of the token. Empty if we have ft_event_kind 'MINT'.
    token_old_owner_account_id            text           NOT NULL DEFAULT '',
    -- New owner of the token. Empty if we have ft_event_kind 'BURN'.
    token_new_owner_account_id            text           NOT NULL DEFAULT '',
    -- Optional message associated with token movement.
    event_memo                            text           NOT NULL DEFAULT ''
);

-- We have to add everything to PK because of some reasons:
-- 1. We need to ignore the same lines, they could come from different indexers, that is fully legal context.
-- 2. We need to catch the situation when we passed PK constraint, but failed UNIQUE constraint below.
ALTER TABLE ONLY fungible_token_events
    ADD CONSTRAINT fungible_token_events_pkey PRIMARY KEY (
    emitted_at_block_timestamp,
    emitted_in_shard_id,
    emitted_index_of_event_entry_in_shard);

CREATE INDEX fungible_token_events_sorting_idx ON fungible_token_events
    USING btree (emitted_at_block_timestamp,
    emitted_in_shard_id,
    emitted_index_of_event_entry_in_shard);

CREATE INDEX fungible_token_events_block_timestamp_idx ON fungible_token_events
    USING btree (emitted_at_block_timestamp);

CREATE INDEX fungible_token_events_old_owner_account_id_idx ON fungible_token_events
    USING btree (token_old_owner_account_id);

CREATE INDEX fungible_token_events_new_owner_account_id_idx ON fungible_token_events
    USING btree (token_new_owner_account_id);

ALTER TABLE ONLY fungible_token_events
    ADD CONSTRAINT fungible_token_events_fk
    FOREIGN KEY (emitted_for_receipt_id) REFERENCES receipts (receipt_id) ON DELETE CASCADE;
