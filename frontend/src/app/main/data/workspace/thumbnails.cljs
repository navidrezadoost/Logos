;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.main.data.workspace.thumbnails
  (:require
   [app.common.data.macros :as dm]
   [app.common.files.geometry-affecting :as gaf]
   [app.common.files.helpers :as cfh]
   [app.common.logging :as l]
   [app.common.thumbnails :as thc]
   [app.common.time :as ct]
   [app.common.types.component :as ctc]
   [app.common.uuid :as uuid]
   [app.main.data.changes :as dch]
   [app.main.data.helpers :as dsh]
   [app.main.data.persistence :as-alias dps]
   [app.main.data.workspace.notifications :as-alias wnt]
   [app.main.data.workspace.pages :as-alias dwpg]
   [app.main.data.workspace.thumbnail-debounce :as tbd]
   [app.main.rasterizer :as thr]
   [app.main.refs :as refs]
   [app.main.render :as render]
   [app.main.repo :as rp]
   [app.main.store :as st]
   [app.util.queue :as q]
   [app.util.timers :as tm]
   [app.util.webapi :as wapi]
   [beicon.v2.core :as rx]
   [cuerdas.core :as str]
   [potok.v2.core :as ptk]))

(declare extract-frame-changes)

(l/set-level! :warn)

(defn- find-request
  [params item]
  (and (= (unchecked-get params "file-id")
          (unchecked-get item "file-id"))
       (= (unchecked-get params "page-id")
          (unchecked-get item "page-id"))
       (= (unchecked-get params "shape-id")
          (unchecked-get item "shape-id"))
       (= (unchecked-get params "tag")
          (unchecked-get item "tag"))))

(defn- create-request
  "Creates a request to generate a thumbnail for the given ids."
  [file-id page-id shape-id tag]
  #js {:file-id file-id
       :page-id page-id
       :shape-id shape-id
       :tag tag})

;; Defines the thumbnail queue
(defonce queue
  (q/create find-request (/ 1000 30)))

(defn clear-queue!
  []
  (l/dbg :hint "clearing thumbnail queue")
  (q/clear! queue))

;; This function first renders the HTML calling `render/render-frame` that
;; returns HTML as a string, then we send that data to the iframe rasterizer
;; that returns the image as a Blob. Finally we create a URI for that blob.
(defn- render-thumbnail
  "Returns the thumbnail for the given ids"
  [state file-id page-id frame-id tag]
  (let [object-id (thc/fmt-object-id file-id page-id frame-id tag)
        tp        (ct/tpoint-ms)
        objects   (-> (dsh/lookup-file-data state file-id)
                      (dsh/get-page page-id)
                      :objects)
        shape     (get objects frame-id)]

    (->> (render/render-frame objects shape object-id)
         (rx/take 1)
         (rx/filter some?)
         (rx/mapcat thr/render)
         (rx/tap #(l/dbg :hint "thumbnail rendered"
                         :elapsed (dm/str (tp) "ms"))))))

(defn- request-thumbnail
  "Enqueues a request to generate a thumbnail for the given ids."
  [state file-id page-id shape-id tag]
  (let [request (create-request file-id page-id shape-id tag)]
    (q/enqueue-unique queue request (partial render-thumbnail state file-id page-id shape-id tag))))

(defn clear-thumbnail
  ([file-id page-id frame-id tag]
   (clear-thumbnail file-id (thc/fmt-object-id file-id page-id frame-id tag)))
  ([file-id object-id]
   (let [pending (volatile! false)]
     (ptk/reify ::clear-thumbnail
       cljs.core/IDeref
       (-deref [_] object-id)

       ptk/UpdateEvent
       (update [_ state]
         (update state :thumbnails
                 (fn [thumbs]
                   (if-let [uri (get thumbs object-id)]
                     (do (vreset! pending uri)
                         (dissoc thumbs object-id))
                     thumbs))))

       ptk/WatchEvent
       (watch [_ _ _]
         (if-let [uri @pending]
           (do
             (l/trc :hint "clear-thumbnail" :uri uri)
             (when (str/starts-with? uri "blob:")
               (tm/schedule-on-idle (partial wapi/revoke-uri uri)))

             (let [params {:file-id file-id
                           :object-id object-id}]
               (->> (rp/cmd! :delete-file-object-thumbnail params)
                    (rx/catch rx/empty)
                    (rx/ignore))))
           (rx/empty)))))))

(defn- assoc-thumbnail
  [object-id uri]
  (let [prev-uri* (volatile! nil)]
    (ptk/reify ::assoc-thumbnail
      ptk/UpdateEvent
      (update [_ state]
        (let [prev-uri (dm/get-in state [:thumbnails object-id])]
          (some->> prev-uri (vreset! prev-uri*))
          (l/trc :hint "assoc thumbnail" :object-id object-id :uri uri)
          (update state :thumbnails assoc object-id uri)))

      ptk/EffectEvent
      (effect [_ _ _]
        (tm/schedule-on-idle
         (fn []
           (when-let [uri (deref prev-uri*)]
             (wapi/revoke-uri uri))))))))

(defn duplicate-thumbnail
  [old-id new-id]
  (ptk/reify ::duplicate-thumbnail
    ptk/UpdateEvent
    (update [_ state]
      (let [old-id (dm/str old-id)
            new-id (dm/str new-id)
            thumbnail (dm/get-in state [:thumbnails old-id])]
        (update state :thumbnails assoc new-id thumbnail)))))

(defn update-thumbnail
  "Updates the thumbnail information for the given `id`"
  [file-id page-id frame-id tag requester]
  (let [object-id (thc/fmt-object-id file-id page-id frame-id tag)]
    (ptk/reify ::update-thumbnail
      cljs.core/IDeref
      (-deref [_] object-id)

      ptk/WatchEvent
      (watch [_ state stream]
        (l/dbg :hint "update thumbnail" :requester requester :object-id object-id :tag tag)
        (let [tp (ct/tpoint-ms)]
          ;; Send the update to the back-end
          (->> (request-thumbnail state file-id page-id frame-id tag)
               (rx/mapcat (fn [blob]
                            (let [uri    (wapi/create-uri blob)
                                  params {:file-id file-id
                                          :object-id object-id
                                          :media blob
                                          :tag (or tag "frame")}]

                              (rx/merge
                               (rx/of (assoc-thumbnail object-id uri))
                               (->> (rp/cmd! :create-file-object-thumbnail params)
                                    (rx/catch rx/empty)
                                    (rx/ignore))))))

               (rx/catch (fn [cause]
                           (.error js/console cause)
                           (rx/empty)))

               (rx/tap #(l/dbg :hint "thumbnail updated" :elapsed (dm/str (tp) "ms")))

               ;; We cancel all the stream if user starts editing while
               ;; thumbnail is generating
               (rx/take-until
                (->> stream
                     (rx/filter (ptk/type? ::clear-thumbnail))
                     (rx/filter #(= (deref %) object-id))))))))))

(defn- extract-frame-changes-v2
  "Like `extract-frame-changes` but tags each result with whether the commit
  contained a geometry-affecting operation.  The geometry? flag is commit-level:
  if any change in the commit touches geometry attributes, every affected frame
  in that commit is considered geometry-dirty.  This is a conservative
  approximation that avoids per-operation bookkeeping."
  [page-id [event [old-data new-data]]]
  (let [geometry? (gaf/geometry-affecting-changes? (:changes event))
        frame-ids  (extract-frame-changes page-id [event [old-data new-data]])]
    (into #{} (map (fn [[tag fid]] {:tag tag :frame-id fid :geometry? geometry?})) frame-ids)))

(defn- extract-frame-changes
  "Process a changes set in a commit to extract the frames that are changing"
  [page-id [event [old-data new-data]]]

  (let [changes (:changes event)
        ;; cache for the get-frame-ids function
        frame-id-cache (atom {})]

    (letfn [(lookup-data-objects [data page-id]
              (dm/get-in data [:pages-index page-id :objects]))

            (extract-ids [{:keys [page-id type] :as change}]
              (case type
                :add-obj [[page-id (:id change)]]
                :mod-obj [[page-id (:id change)]]
                :del-obj [[page-id (:id change)]]
                :mov-objects (->> (:shapes change) (map #(vector page-id %)))
                []))

            (get-frame-ids [id]
              (let [old-objects     (lookup-data-objects old-data page-id)
                    new-objects     (lookup-data-objects new-data page-id)

                    new-shape       (get new-objects id)
                    old-shape       (get old-objects id)

                    old-frame-id    (if (cfh/frame-shape? old-shape) id (:frame-id old-shape))
                    new-frame-id    (if (cfh/frame-shape? new-shape) id (:frame-id new-shape))

                    root-frame-old? (cfh/root-frame? old-objects old-frame-id)
                    root-frame-new? (cfh/root-frame? new-objects new-frame-id)
                    instance-root?  (ctc/instance-root? new-shape)]

                (cond-> #{}
                  root-frame-old?
                  (conj ["frame" old-frame-id])

                  root-frame-new?
                  (conj ["frame" new-frame-id])

                  instance-root?
                  (conj ["component" id])

                  (and (uuid? (:frame-id old-shape))
                       (not= uuid/zero (:frame-id old-shape)))
                  (into (get-frame-ids (:frame-id old-shape)))

                  (and (uuid? (:frame-id new-shape))
                       (not= uuid/zero (:frame-id new-shape)))
                  (into (get-frame-ids (:frame-id new-shape))))))

            (get-frame-ids-cached [id]
              (or (get @frame-id-cache id)
                  (let [result (get-frame-ids id)]
                    (swap! frame-id-cache assoc id result)
                    result)))]
      (into #{}
            (comp (mapcat extract-ids)
                  (filter (fn [[page-id']] (= page-id page-id')))
                  (map (fn [[_ id]] id))
                  (mapcat get-frame-ids-cached))
            changes))))

(defn watch-state-changes
  "Watch the state for changes inside frames. If a change is detected will force
  a rendering of the frame data so the thumbnail can be updated.

  Optimised path (P1.4):
  - Geometry-affecting changes (add/del/mov shapes, geometry attr edits) trigger
    an immediate `clear-thumbnail` so the sidebar never shows stale content, then
    schedule a per-frame 2-second debounced re-render.
  - Style-only changes (colour, shadow, …) skip the immediate clear and only
    schedule the debounced re-render — the existing thumbnail remains visible
    until the regeneration fires.
  - Each frame has its own independent debounce timer; a new change for frame A
    does not reset the timer for frame B.
  - On page finalisation all pending timers are cancelled to avoid dispatching
    events against a dead page."
  [file-id page-id]
  (ptk/reify ::watch-state-changes
    ptk/WatchEvent
    (watch [_ _ stream]
      (let [stopper-s (rx/filter
                       (fn [event]
                         (as-> (ptk/type event) type
                           (or (= ::dwpg/finalize-page type)
                               (= ::watch-state-changes type))))
                       stream)

            workspace-data-s
            (->> (rx/concat
                  (rx/of nil)
                  (rx/from-atom refs/workspace-data {:emit-current-value? true}))
                 ;; We need to keep the old-objects so we can check the frame for
                 ;; deleted objects
                 (rx/buffer 2 1)
                 (rx/share))

            ;; All commits stream — emits {:tag, :frame-id, :geometry?} maps
            all-commits-s
            (->> stream
                 (rx/filter dch/commit?)
                 (rx/map deref)
                 (rx/observe-on :async)
                 (rx/with-latest-from workspace-data-s)
                 (rx/merge-map #(rx/from (extract-frame-changes-v2 page-id %)))
                 (rx/tap #(l/trc :hint "incoming change"
                                 :origin "all"
                                 :frame-id (dm/str (:frame-id %))
                                 :geometry? (:geometry? %)))
                  (rx/share))

                clear-geometry-s
                (->> all-commits-s
                  (rx/filter :geometry?)
                  (rx/mapcat (fn [{:keys [tag frame-id]}]
                      (rx/of (clear-thumbnail file-id page-id frame-id tag)))))

                debounce-s
                (->> all-commits-s
                  (rx/tap (fn [{:keys [tag frame-id]}]
                      (let [job-key [file-id page-id frame-id tag]]
                        (tbd/schedule-update!
                      job-key
                      2000
                      (fn []
                        (tbd/complete-job! job-key)
                        (st/emit!
                         (update-thumbnail file-id page-id frame-id tag
                               "debounced")))))))
                  (rx/ignore))

                cleanup-s
                (->> stream
                  (rx/filter (ptk/type? ::dwpg/finalize-page))
                  (rx/take 1)
                  (rx/tap (fn [_] (tbd/clear-all!)))
                  (rx/ignore))]

               (->> (rx/merge clear-geometry-s debounce-s cleanup-s)
                 (rx/take-until stopper-s))))))
