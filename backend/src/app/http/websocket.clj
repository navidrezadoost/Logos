;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.http.websocket
  "A penpot notification service for file cooperative edition."
  (:require
   [app.common.exceptions :as ex]
   [app.common.logging :as l]
   [app.common.pprint :as pp]
   [app.common.schema :as sm]
   [app.common.time :as ct]
   [app.common.uuid :as uuid]
   [app.db :as db]
   [app.http.session :as session]
   [app.metrics :as mtx]
   [app.msgbus :as mbus]
   [app.util.websocket :as ws]
   [integrant.core :as ig]
   [promesa.exec.csp :as sp]
   [yetti.websocket :as yws]))

(def recv-labels
  (into-array String ["recv"]))

(def send-labels
  (into-array String ["send"]))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; WEBSOCKET HOOKS
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(def state (atom {}))

;; REPL HELPERS

(defn repl-get-connections-for-file
  [file-id]
  (->> (vals @state)
       (filter #(= file-id (-> % deref ::file-subscription :file-id)))
       (map ::ws/id)))

(defn repl-get-connections-for-team
  [team-id]
  (->> (vals @state)
       (filter #(= team-id (-> % deref ::team-subscription :team-id)))
       (map ::ws/id)))

(defn repl-close-connection
  [id]
  (when-let [{:keys [::ws/close-ch] :as wsp} (get @state id)]
    (sp/put! close-ch [8899 "closed from server"])
    (sp/close! close-ch)))

(defn repl-get-connection-info
  [id]
  (when-let [wsp (get @state id)]
    {:id               id
     :created-at       (::created-at wsp)
     :profile-id       (::profile-id wsp)
     :session-id       (::session-id wsp)
     :user-agent       (::ws/user-agent wsp)
     :ip-addr          (::ws/remote-addr wsp)
     :last-activity-at (::ws/last-activity-at wsp)
     :subscribed-file  (-> wsp ::file-subscription :file-id)
     :subscribed-team  (-> wsp ::team-subscription :team-id)}))

(defn repl-print-connection-info
  [id]
  (some-> id repl-get-connection-info pp/pprint))

(defn repl-print-connection-info-for-file
  [file-id]
  (some->> (repl-get-connections-for-file file-id)
           (map repl-get-connection-info)
           (pp/pprint)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; WEBSOCKET HANDLER
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(defmulti handle-message
  (fn [_ _ message]
    (:type message)))

(defmethod handle-message :open
  [{:keys [::mbus/msgbus]} {:keys [::ws/id ::ws/output-ch ::ws/state ::profile-id ::session-id] :as wsp} _]
  (l/trace :fn "handle-message" :event "open" :conn-id id)
  (let [ch (sp/chan :buf (sp/dropping-buffer 16)
                    :xf  (remove #(= (:session-id %) session-id)))]

    ;; Subscribe to the profile channel and forward all messages to websocket output
    ;; channel (send them to the client).
    (swap! state assoc ::profile-subscription {:channel ch})

    ;; Forward the subscription messages directly to the websocket output channel
    (sp/pipe ch output-ch false)

    ;; Subscribe to the profile topic on msgbus/redis
    (mbus/sub! msgbus :topic profile-id :chan ch)

    ;; Subscribe to the system topic on msgbus/redis
    (mbus/sub! msgbus :topic (str uuid/zero) :chan ch)))

(defmethod handle-message :close
  [{:keys [::mbus/msgbus]} {:keys [::ws/id ::ws/state ::profile-id ::session-id]} _]
  (l/trace :fn "handle-message" :event "close" :conn-id id)
  (let [psub (::profile-subscription @state)
        fsub (::file-subscription @state)
        tsub (::team-subscription @state)
        msg  {:type :disconnect
              :profile-id profile-id
              :session-id session-id}]

    ;; Close profile subscription if exists
    (when-let [ch (:channel psub)]
      (sp/close! ch)
      (mbus/purge! msgbus [ch]))

    ;; Close team subscription if exists
    (when-let [ch (:channel tsub)]
      (sp/close! ch)
      (mbus/purge! msgbus [ch]))

    ;; Close file subscription if exists
    (when-let [{:keys [topic channel file-id]} fsub]
      (sp/close! channel)
      (mbus/purge! msgbus [channel])
      ;; Notify legacy and meta listeners of disconnection
      (mbus/pub! msgbus :topic topic                          :message msg)
      (mbus/pub! msgbus :topic (mbus/file-meta-topic file-id) :message msg))))

(defmethod handle-message :subscribe-team
  [{:keys [::mbus/msgbus]} {:keys [::ws/id ::ws/state ::ws/output-ch ::session-id]} {:keys [team-id] :as params}]
  (l/trace :fn "handle-message" :event "subscribe-team" :team-id team-id :conn-id id)
  (let [prev-subs (get @state ::team-subscription)
        channel   (sp/chan :buf (sp/dropping-buffer 64)
                           :xf  (remove #(= (:session-id %) session-id)))]

    (sp/pipe channel output-ch false)
    (mbus/sub! msgbus :topic team-id :chan channel)

    (let [subs {:team-id team-id :channel channel :topic team-id}]
      (swap! state assoc ::team-subscription subs))

    ;; Close previous subscription if exists
    (when-let [ch (:channel prev-subs)]
      (sp/close! ch)
      (mbus/purge! msgbus [ch]))))


(defmethod handle-message :subscribe-file
  [{:keys [::mbus/msgbus]} {:keys [::ws/id ::ws/state ::ws/output-ch ::session-id ::profile-id]} {:keys [file-id page-id] :as params}]
  (l/trace :fn "handle-message" :event "subscribe-file" :file-id file-id :page-id page-id :conn-id id)
  (let [psub (::file-subscription @state)
        fch  (sp/chan :buf (sp/dropping-buffer 64)
                      :xf  (remove #(= (:session-id %) session-id)))]

    ;; P2.1: when page-id is provided, subscribe the channel to both the
    ;; page-scoped topic (full change-sets) and the file-meta topic (presence,
    ;; pointer-updates, :page-updated notifications from OTHER pages).
    ;; Without page-id, keep the legacy file-id subscription for backward compat.
    (let [topics      (if page-id
                        [(mbus/page-topic file-id page-id)
                         (mbus/file-meta-topic file-id)]
                        [file-id])
          topic-for-presence (if page-id
                               (mbus/file-meta-topic file-id)
                               file-id)
          subs        {:file-id          file-id
                       :page-id          page-id
                       :channel          fch
                       :topic            (first topics)
                       :presence-topic   topic-for-presence}]

      (swap! state assoc ::file-subscription subs)

      ;; Close previous subscription channel if exists
      (when-let [ch (:channel psub)]
        (sp/close! ch)
        (mbus/purge! msgbus [ch]))

      (sp/go-loop []
        (when-let [{:keys [type] :as message} (sp/take! fch)]
          (sp/put! output-ch message)
          (when (or (= :join-file type)
                    (= :leave-file type)
                    (= :disconnect type))
            (let [message {:type :presence
                           :file-id file-id
                           :session-id session-id
                           :profile-id profile-id}]
              ;; Publish presence to both legacy and meta topics
              (mbus/pub! msgbus :topic file-id              :message message)
              (mbus/pub! msgbus :topic (mbus/file-meta-topic file-id) :message message)))
          (recur)))

      ;; Subscribe the channel to the resolved topics
      (mbus/sub! msgbus :topics topics :chan fch)

      ;; Notify other participants of the new connection
      (let [message {:type :join-file
                     :file-id file-id
                     :session-id session-id
                     :profile-id profile-id}]
        (mbus/pub! msgbus :topic file-id              :message message)
        (mbus/pub! msgbus :topic (mbus/file-meta-topic file-id) :message message)))))

(defmethod handle-message :unsubscribe-file
  [{:keys [::mbus/msgbus]} {:keys [::ws/id ::ws/state ::session-id ::profile-id]} {:keys [file-id] :as params}]
  (l/trace :fn "handle-message" :event "unsubscribe-file" :file-id file-id :conn-id id)

  (let [subs    (::file-subscription @state)
        message {:type :leave-file
                 :file-id file-id
                 :session-id session-id
                 :profile-id profile-id}]

    (when (= (:file-id subs) file-id)
      ;; Publish leave to both legacy and meta topics so all client types receive it
      (mbus/pub! msgbus :topic file-id                          :message message)
      (mbus/pub! msgbus :topic (mbus/file-meta-topic file-id)  :message message)
      (let [ch (:channel subs)]
        (sp/close! ch)
        (mbus/purge! msgbus [ch])))))

(defmethod handle-message :keepalive
  [_ _ _]
  (l/trace :fn "handle-message" :event :keepalive))

(defmethod handle-message :broadcast
  [{:keys [::mbus/msgbus]} {:keys [::ws/id ::session-id ::profile-id]} message]
  (l/trace :fn "handle-message" :event "broadcast" :conn-id id)
  (let [message (-> message
                    (assoc :subs-id profile-id)
                    (assoc :profile-id profile-id)
                    (assoc :session-id session-id))]
    (mbus/pub! msgbus :topic profile-id :message message)))

(defmethod handle-message :pointer-update
  [{:keys [::mbus/msgbus]} {:keys [::ws/state ::session-id ::profile-id]} {:keys [file-id] :as message}]
  (when (::file-subscription @state)
    (let [message (-> message
                      (assoc :subs-id file-id)
                      (assoc :profile-id profile-id)
                      (assoc :session-id session-id))]
      ;; Publish to both legacy topic and meta topic so all client types receive cursor updates
      (mbus/pub! msgbus :topic file-id                         :message message)
      (mbus/pub! msgbus :topic (mbus/file-meta-topic file-id) :message message))))

(defmethod handle-message :default
  [_ {:keys [::ws/id]} message]
  (l/warn :hint "received unexpected message"
          :message message
          :conn-id id))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; HTTP HANDLER
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(defn- on-connect
  [{:keys [::mtx/metrics]} {:keys [::ws/id] :as wsp}]
  (let [created-at (ct/now)]
    (l/trace :fn "on-connect" :conn-id id)
    (swap! state assoc id wsp)
    (mtx/run! metrics
              :id :websocket-active-connections
              :inc 1)

    (assoc wsp ::ws/on-disconnect
           (fn []
             (l/trace :fn "on-disconnect" :conn-id id)
             (swap! state dissoc id)
             (mtx/run! metrics :id :websocket-active-connections :dec 1)
             (mtx/run! metrics
                       :id :websocket-session-timing
                       :val (/ (inst-ms (ct/diff created-at (ct/now))) 1000.0))))))

(defn- on-rcv-message
  [{:keys [::mtx/metrics ::profile-id ::session-id]} message]
  (mtx/run! metrics
            :id :websocket-messages-total
            :labels recv-labels
            :inc 1)
  (assoc message :profile-id profile-id :session-id session-id))

(defn- on-snd-message
  [{:keys [::mtx/metrics]} message]
  (mtx/run! metrics
            :id :websocket-messages-total
            :labels send-labels
            :inc 1)
  message)

(defn- http-handler
  [cfg {:keys [params ::session/profile-id] :as request}]
  (let [session-id (some-> params :session-id uuid/parse*)]
    (when-not (uuid? session-id)
      (ex/raise :type :validation
                :code :missing-session-id
                :hint "missing or invalid session-id found"))

    (cond
      (not profile-id)
      (ex/raise :type :authentication
                :hint "authentication required")

      ;; WORKAROUND: we use the adapter specific predicate for
      ;; performance reasons; for now, the ring default impl for
      ;; `upgrade-request?` parses all requests headers before perform
      ;; any checking.
      (not (yws/upgrade-request? request))
      (ex/raise :type :validation
                :code :websocket-request-expected
                :hint "this endpoint only accepts websocket connections")

      :else
      (do
        (l/trace :hint "websocket request" :profile-id profile-id :session-id session-id)
        {::yws/listener (ws/listener request
                                     ::ws/on-rcv-message (partial on-rcv-message cfg)
                                     ::ws/on-snd-message (partial on-snd-message cfg)
                                     ::ws/on-connect (partial on-connect cfg)
                                     ::ws/handler (partial handle-message cfg)
                                     ::profile-id profile-id
                                     ::session-id session-id)}))))


(def ^:private schema:routes-params
  [:map
   ::mbus/msgbus
   ::mtx/metrics
   ::db/pool
   ::session/manager])

(defmethod ig/assert-key ::routes
  [_ params]
  (assert (sm/valid? schema:routes-params params)))

(defmethod ig/init-key ::routes
  [_ cfg]
  ["/ws/notifications" {:middleware [[session/authz cfg]]
                        :handler (partial http-handler cfg)}])
