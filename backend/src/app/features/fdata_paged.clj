;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.features.fdata-paged
  "P2.2 — Page-level file data fragmentation.

  Instead of reading / writing the entire file JSONB blob for every
  change, each page is stored as an independent row in the
  `file_data_page` table.  The monolithic `file.data` column continues
  to work as the authoritative source for files that have not yet been
  migrated (storage_format = 'monolithic').

  Migration discipline
  ────────────────────
  • Files start as 'monolithic'.
  • The background `offload-file-data` task (extended in
    app.tasks.offload-file-data) promotes them to 'paged' by splitting
    the existing data blob into per-page rows.
  • All read/write code checks storage_format and falls back to the
    monolithic path when the file has not been promoted yet.
  • No test assertions change — the fragmentation is a storage detail."
  (:require
   [app.common.logging :as l]
   [app.db :as db]
   [app.util.blob :as blob]))

;; ──────────────────────────────────────────────────────────────────
;; SQL
;; ──────────────────────────────────────────────────────────────────

(def ^:private sql:get-page-data
  "SELECT data, revn
     FROM file_data_page
    WHERE file_id = ?
      AND page_id = ?")

(def ^:private sql:upsert-page-data
  "INSERT INTO file_data_page (file_id, page_id, data, revn, updated_at)
   VALUES (?, ?, ?, ?, now())
   ON CONFLICT (file_id, page_id)
   DO UPDATE SET data       = EXCLUDED.data,
                 revn       = EXCLUDED.revn,
                 updated_at = now()
   WHERE file_data_page.revn <= EXCLUDED.revn")

(def ^:private sql:delete-page-row
  "DELETE FROM file_data_page
    WHERE file_id = ?
      AND page_id = ?")

(def ^:private sql:list-page-ids
  "SELECT page_id
     FROM file_data_page
    WHERE file_id = ?
    ORDER BY page_id")

(def ^:private sql:promote-file
  "UPDATE file
      SET storage_format = 'paged',
          modified_at    = now()
    WHERE id = ?
      AND storage_format = 'monolithic'")

(def ^:private sql:bump-current-revn
  "UPDATE file
      SET current_revn = current_revn + 1,
          modified_at  = now()
    WHERE id = ?
   RETURNING current_revn")

;; ──────────────────────────────────────────────────────────────────
;; Public API
;; ──────────────────────────────────────────────────────────────────

(defn paged?
  "Return true when the file is stored in the paged format."
  [file]
  (= "paged" (:storage-format file)))

(defn get-page-data
  "Return a decoded page map (same shape as an entry in :pages-index)
  for the given file-id + page-id.  Returns nil when the page row does
  not exist yet (e.g. file not yet migrated or page not yet written)."
  [{:keys [::db/conn] :as _cfg} file-id page-id]
  (when-let [{:keys [data revn]}
             (db/exec-one! conn [sql:get-page-data file-id page-id])]
    {:page (blob/decode data)
     :revn revn}))

(defn save-page-data!
  "Persist a single page's shape tree.  Uses an UPSERT with an optimistic
  revn guard so a stale write never overwrites a fresher one.

  Returns the new server `current_revn` for the file."
  [{:keys [::db/conn] :as _cfg} file-id page-id page revn]
  (let [encoded (blob/encode page)]
    (db/exec! conn [sql:upsert-page-data
                    file-id page-id encoded (inc revn)])
    ;; Advance the file-level monotonic revision counter (prereq for P2.3)
    (-> (db/exec-one! conn [sql:bump-current-revn file-id])
        :current-revn)))

(defn delete-page-data!
  "Remove a page row.  Called when a page is deleted from the file."
  [{:keys [::db/conn] :as _cfg} file-id page-id]
  (db/exec! conn [sql:delete-page-row file-id page-id]))

(defn list-page-ids
  "Return the seq of page-ids that have been fragmented for file-id."
  [{:keys [::db/conn] :as _cfg} file-id]
  (->> (db/exec! conn [sql:list-page-ids file-id])
       (mapv :page-id)))

;; ──────────────────────────────────────────────────────────────────
;; Load helpers  (used by get-page read path)
;; ──────────────────────────────────────────────────────────────────

(defn load-page
  "High-level helper: load a single page from `file_data_page` when the
  file is in paged format, or return nil so the caller can fall back to
  the monolithic path."
  [cfg file-id page-id]
  (try
    (get-page-data cfg file-id page-id)
    (catch Exception cause
      (l/warn :hint "fdata-paged/load-page failed; falling back to monolithic"
              :file-id (str file-id)
              :page-id (str page-id)
              :cause cause)
      nil)))

;; ──────────────────────────────────────────────────────────────────
;; Migration helpers  (called from offload-file-data task)
;; ──────────────────────────────────────────────────────────────────

(defn fragment-file!
  "Split a monolithic file's page data into per-page rows.

  `file` must already have its `:data` decoded (keys :pages-index).
  Each page is written at revn 0 (initial import).  After all pages
  are written the `storage_format` column is flipped to 'paged'."
  [{:keys [::db/conn] :as cfg} {:keys [id data] :as _file}]
  (let [pages-index (:pages-index data)
        page-ids    (keys pages-index)]
    (doseq [page-id page-ids]
      (let [page    (get pages-index page-id)
            encoded (blob/encode page)]
        (db/exec! conn [sql:upsert-page-data id page-id encoded 0])))
    ;; Atomically promote the file
    (db/exec! conn [sql:promote-file id])
    (l/dbg :hint "file fragmented into per-page rows"
           :file-id (str id)
           :page-count (count page-ids))))
