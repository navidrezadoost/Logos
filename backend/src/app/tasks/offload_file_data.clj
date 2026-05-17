;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.tasks.offload-file-data
  "A maintenance task responsible of moving file data from hot
  storage (the database row) to a cold storage (fs or s3).

  Extended in P2.2 to also fragment monolithic files into the
  per-page `file_data_page` table (storage_format = 'paged').
  Fragmentation runs inside the same transaction so callers can
  pass rollback? = true for dry-run testing."
  (:require
   [app.binfile.common :as bfc]
   [app.common.logging :as l]
   [app.db :as db]
   [app.features.fdata :as fdata]
   [app.features.fdata-paged :as fdata-paged]
   [app.storage :as sto]
   [integrant.core :as ig]))

(def ^:private sql:get-file-data
  "SELECT fd.*
     FROM file_data AS fd
    WHERE fd.file_id = ?
      AND fd.backend = 'db'
      AND fd.deleted_at IS NULL")

(defn- offload-file-data
  [cfg {:keys [id file-id type] :as fdata}]
  (fdata/upsert! cfg (assoc fdata :backend "storage"))
  (l/trc :file-id (str file-id)
         :id (str id)
         :type type))

;; ──────────────────────────────────────────────────────────────────
;; P2.2 — page fragmentation step
;; ──────────────────────────────────────────────────────────────────

(defn- fragment-file-pages!
  "Promote a single monolithic file to per-page row storage.

  The file is loaded fully (pointer maps resolved), then
  `fdata-paged/fragment-file!` writes one row per page and flips the
  storage_format flag to 'paged' — all within the caller's
  transaction.

  Skips files that are already in paged format or exceed the automatic
  size limit (C2 guard — see fdata_paged/max-auto-fragment-bytes)."
  [cfg file-id]
  (let [file (bfc/get-file cfg file-id :read-only? false)]
    (if (fdata-paged/paged? file)
      (l/dbg :hint "skipping already-paged file" :file-id (str file-id))
      (let [realized (fdata/realize cfg file)
            result   (fdata-paged/fragment-file! cfg realized)]
        (when (= :skipped-too-large result)
          (l/warn :hint "file skipped for automatic fragmentation (size limit)"
                  :file-id (str file-id)))))))


;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; HANDLER
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(defmethod ig/assert-key ::handler
  [_ params]
  (assert (db/pool? (::db/pool params)) "expected a valid database pool")
  (assert (sto/valid-storage? (::sto/storage params)) "expected valid storage to be provided"))

(defmethod ig/init-key ::handler
  [_ cfg]
  (fn [{:keys [props] :as task}]
    (let [file-id         (:file-id props)
          fragment-pages? (boolean (:fragment-pages? props true))]
      (-> cfg
          (assoc ::db/rollback (:rollback? props))
          (db/tx-run! (fn [{:keys [::db/conn] :as cfg}]
                        ;; Step 1: existing binary offload (unchanged)
                        (run! (partial offload-file-data cfg)
                              (db/plan conn [sql:get-file-data file-id]))
                        ;; Step 2: P2.2 per-page fragmentation (opt-out with
                        ;; :fragment-pages? false in task props)
                        (when fragment-pages?
                          (fragment-file-pages! cfg file-id))))))))
