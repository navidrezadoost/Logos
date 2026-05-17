-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.
--
-- Copyright (c) KALEIDOS INC

-- Add per-page file data table for P2.2 (page-level fragmentation).
-- Stores each page's shape tree as an independent row, enabling O(page)
-- reads/writes instead of O(file) for single-page edits.

CREATE TABLE file_data_page (
    file_id    uuid        NOT NULL REFERENCES file(id) ON DELETE CASCADE DEFERRABLE,
    page_id    uuid        NOT NULL,
    data       bytea       NOT NULL,
    revn       bigint      NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (file_id, page_id)
);

CREATE INDEX file_data_page__file_id__idx
    ON file_data_page (file_id);

-- storage_format: 'monolithic' = classic single-blob file.data column,
--                 'paged'      = per-page rows in file_data_page.
-- Migration is incremental (background task), so both coexist.
ALTER TABLE file
    ADD COLUMN storage_format text NOT NULL DEFAULT 'monolithic',
    ADD COLUMN current_revn   bigint NOT NULL DEFAULT 0;

CREATE INDEX file__storage_format__idx
    ON file (storage_format)
 WHERE storage_format = 'monolithic';

-- P2.3 prerequisites: operational-transform revision tracking on file_change.
-- base_revn   = the file revn this change-set was built against (client-reported)
-- server_revn = the revn assigned by the server when the change-set was applied
-- rebased     = true when the server had to rebase the change-set before applying
ALTER TABLE file_change
    ADD COLUMN IF NOT EXISTS base_revn   bigint  NULL,
    ADD COLUMN IF NOT EXISTS server_revn bigint  NULL,
    ADD COLUMN IF NOT EXISTS rebased     boolean NOT NULL DEFAULT false;
