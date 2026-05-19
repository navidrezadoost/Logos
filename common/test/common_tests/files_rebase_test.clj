;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns common-tests.files-rebase-test
  "P2.3 — Property-based and unit tests for the OT rebase engine.

  Test categories
  ───────────────
  1. Identity / idempotency
     - rebase against empty competing sequence → identity
     - rebase-change-set [] … → []

  2. Non-conflicting preservation
     - changes on different shape ids → both preserved

  3. Conflicting :set-attr (via :mod-obj operations)
     - same shape, same attr → incoming wins

  4. Delete semantics
     - del-obj + mod-obj on same id → mod-obj dropped
     - del-obj + del-obj on same id → idempotent (both kept, second is a no-op)

  5. Move / index adjustment
     - mov-objects against add-obj at same parent, index ≤ target → index +1
     - mov-objects shapes list pruned when competing del-obj deletes a shape

  6. Cross-page isolation
     - changes on different page-ids don't interfere (dispatch is by :id not :page-id)"
  (:require
   [app.common.files.rebase :as rebase]
   [app.common.uuid :as uuid]
   [clojure.test :as t]))

;; ──────────────────────────────────────────────────────────────────
;; Fixtures
;; ──────────────────────────────────────────────────────────────────

(def ^:private id-a (uuid/custom 1 1))
(def ^:private id-b (uuid/custom 1 2))
(def ^:private id-c (uuid/custom 1 3))
(def ^:private parent-id (uuid/custom 2 1))
(def ^:private page-id-1 (uuid/custom 3 1))
(def ^:private page-id-2 (uuid/custom 3 2))

(defn- set-op
  ([attr val] {:type :set :attr attr :val val})
  ([attr val] {:type :set :attr attr :val val}))

(defn- mod-obj
  [id & ops]
  {:type :mod-obj :id id :page-id page-id-1 :operations (vec ops)})

(defn- del-obj
  [id]
  {:type :del-obj :id id :page-id page-id-1})

(defn- add-obj
  ([id] (add-obj id parent-id 0))
  ([id parent index]
   {:type :add-obj :id id :parent-id parent :index index :page-id page-id-1}))

(defn- mov-objects
  [shapes parent index]
  {:type :mov-objects :shapes shapes :parent-id parent :index index :page-id page-id-1})

;; ──────────────────────────────────────────────────────────────────
;; 1. Identity / idempotency
;; ──────────────────────────────────────────────────────────────────

(t/deftest rebase-against-empty-competing-is-identity
  (let [cs [(mod-obj id-a (set-op :name "foo"))
            (del-obj id-b)]]
    (t/is (= cs (rebase/rebase-change-set cs [])))))

(t/deftest rebase-empty-change-set-is-empty
  (t/is (= [] (rebase/rebase-change-set [] [[(del-obj id-a)]]))))

(t/deftest rebase-change-single-no-conflict
  (let [ch (mod-obj id-a (set-op :name "foo"))]
    (t/is (= ch (rebase/rebase-change ch [])))))

;; ──────────────────────────────────────────────────────────────────
;; 2. Non-conflicting preservation
;; ──────────────────────────────────────────────────────────────────

(t/deftest different-shape-ids-both-preserved
  (let [incoming   [(mod-obj id-a (set-op :name "Alice"))]
        competing  [[(mod-obj id-b (set-op :name "Bob"))]]
        rebased    (rebase/rebase-change-set incoming competing)]
    (t/is (= incoming rebased))
    (t/is (= 1 (count rebased)))))

(t/deftest del-obj-on-different-id-preserved
  (let [incoming   [(del-obj id-a)]
        competing  [[(del-obj id-b)]]
        rebased    (rebase/rebase-change-set incoming competing)]
    (t/is (= 1 (count rebased)))))

;; ──────────────────────────────────────────────────────────────────
;; 3. :mod-obj vs :mod-obj — incoming attrs win
;; ──────────────────────────────────────────────────────────────────

(t/deftest mod-obj-same-attr-incoming-wins
  ;; Both clients set :name on the same shape.
  ;; After rebase, the incoming change's value should be preserved.
  (let [incoming    [(mod-obj id-a (set-op :name "incoming-name"))]
        competing   [[(mod-obj id-a (set-op :name "competing-name"))]]
        [rebased-ch] (rebase/rebase-change-set incoming competing)
        ops         (:operations rebased-ch)
        name-val    (->> ops (filter #(= :name (:attr %))) first :val)]
    (t/is (= "incoming-name" name-val))))

(t/deftest mod-obj-different-attrs-both-preserved
  ;; Clients each set a different attribute — both should survive.
  (let [incoming    [(mod-obj id-a (set-op :name "inc-name"))]
        competing   [[(mod-obj id-a (set-op :fill-color "#ff0000"))]]
        [rebased-ch] (rebase/rebase-change-set incoming competing)
        ops         (:operations rebased-ch)
        attrs       (set (map :attr ops))]
    ;; incoming's :name op is always present
    (t/is (contains? attrs :name))
    ;; competing's :fill-color op is also present (merged)
    (t/is (contains? attrs :fill-color))))

;; ──────────────────────────────────────────────────────────────────
;; 4. Delete semantics
;; ──────────────────────────────────────────────────────────────────

(t/deftest mod-obj-dropped-when-competing-del-obj-same-id
  ;; A competing client deleted shape A.  Our incoming modify on A
  ;; should be dropped (the object no longer exists).
  (let [incoming    [(mod-obj id-a (set-op :name "foo"))]
        competing   [[(del-obj id-a)]]
        rebased     (rebase/rebase-change-set incoming competing)]
    (t/is (empty? rebased))))

(t/deftest del-obj-kept-when-competing-del-obj-same-id
  ;; Two clients both delete the same object.  The second delete is
  ;; idempotent — we should preserve it (safe to apply, ignored by server).
  (let [incoming    [(del-obj id-a)]
        competing   [[(del-obj id-a)]]
        rebased     (rebase/rebase-change-set incoming competing)]
    (t/is (= 1 (count rebased)))))

(t/deftest del-obj-preserved-when-competing-mod-obj-same-id
  ;; Our delete wins over a competing mod.
  (let [incoming    [(del-obj id-a)]
        competing   [[(mod-obj id-a (set-op :name "bar"))]]
        rebased     (rebase/rebase-change-set incoming competing)]
    (t/is (= 1 (count rebased)))
    (t/is (= :del-obj (:type (first rebased))))))

(t/deftest add-obj-preserved-after-competing-del-obj-same-id
  ;; Our add-obj on id-a is preserved even if a competing change deleted id-a.
  ;; Re-add wins (collaborative resilience rule).
  (let [incoming    [(add-obj id-a)]
        competing   [[(del-obj id-a)]]
        rebased     (rebase/rebase-change-set incoming competing)]
    (t/is (= 1 (count rebased)))
    (t/is (= :add-obj (:type (first rebased))))))

;; ──────────────────────────────────────────────────────────────────
;; 5. Move / index adjustment
;; ──────────────────────────────────────────────────────────────────

(t/deftest mov-objects-index-incremented-when-competing-add-before
  ;; Competing client added an object at index 1 in our parent.
  ;; Our mov-objects was targeting index 2.
  ;; After rebase, our index should be bumped to 3.
  (let [incoming    [(mov-objects [id-a] parent-id 2)]
        competing   [[(add-obj id-b parent-id 1)]]
        [rebased]   (rebase/rebase-change-set incoming competing)]
    (t/is (= 3 (:index rebased)))))

(t/deftest mov-objects-index-unchanged-when-competing-add-after
  ;; Competing add is at index 5, our target is index 2 — no shift needed.
  (let [incoming    [(mov-objects [id-a] parent-id 2)]
        competing   [[(add-obj id-b parent-id 5)]]
        [rebased]   (rebase/rebase-change-set incoming competing)]
    (t/is (= 2 (:index rebased)))))

(t/deftest mov-objects-shapes-pruned-when-competing-del-obj
  ;; Competing client deleted id-b.  Our mov includes [id-a id-b id-c].
  ;; After rebase, id-b must be pruned.
  (let [incoming  [(mov-objects [id-a id-b id-c] parent-id 0)]
        competing [[(del-obj id-b)]]
        [rebased] (rebase/rebase-change-set incoming competing)]
    (t/is (= [id-a id-c] (:shapes rebased)))))

(t/deftest mov-objects-dropped-when-all-shapes-deleted
  ;; Competing client deleted all shapes in our mov.
  ;; The mov-objects change should be dropped entirely.
  (let [incoming  [(mov-objects [id-a id-b] parent-id 0)]
        competing [[(del-obj id-a) (del-obj id-b)]]
        rebased   (rebase/rebase-change-set incoming competing)]
    (t/is (empty? rebased))))

;; ──────────────────────────────────────────────────────────────────
;; 6. Multiple competing change-sets
;; ──────────────────────────────────────────────────────────────────

(t/deftest rebase-against-multiple-competing-sets
  ;; Three competing revisions, each deleting a different shape.
  ;; Our incoming changes on id-a and id-b should both be dropped.
  ;; Change on id-c (not deleted) should survive.
  (let [incoming    [(mod-obj id-a (set-op :name "a"))
                     (mod-obj id-b (set-op :name "b"))
                     (mod-obj id-c (set-op :name "c"))]
        competing   [[(del-obj id-a)]
                     [(del-obj id-b)]
                     []]
        rebased     (rebase/rebase-change-set incoming competing)]
    (t/is (= 1 (count rebased)))
    (t/is (= id-c (:id (first rebased))))))

(t/deftest rebase-change-set-preserves-order
  ;; Non-conflicting changes should come out in the same order they went in.
  (let [incoming    [(mod-obj id-a (set-op :name "a"))
                     (mod-obj id-b (set-op :name "b"))
                     (mod-obj id-c (set-op :name "c"))]
        [r1 r2 r3]  (rebase/rebase-change-set incoming [])]
    (t/is (= id-a (:id r1)))
    (t/is (= id-b (:id r2)))
    (t/is (= id-c (:id r3)))))
