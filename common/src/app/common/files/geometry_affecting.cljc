;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.files.geometry-affecting
  "Canonical registry of attributes and change-set types that affect the
  geometry (position/size/shape) of a shape and therefore require the
  flex/grid layout engine to re-run.

  Implementations of the Dirty Flag optimisation (P1.3) MUST reference
  this namespace for the single source of truth.  Do not embed inline sets
  in call-sites — always use the exported functions.

  Usage:
    (require '[app.common.files.geometry-affecting :as gaf])

    ;; Test a single change-set level
    (gaf/geometry-affecting-change? change)

    ;; Test a seq of change-sets (returns true if ANY change affects geometry)
    (gaf/geometry-affecting-changes? changes)
  "
  (:require
   [app.common.data :as d]))

;; ---------------------------------------------------------------------------
;; Attribute sets
;; ---------------------------------------------------------------------------

(def ^:const geometry-attrs
  "Shape attributes that directly control geometry / layout behaviour.
  Colour, shadow, blur, opacity, and other purely-visual attrs are NOT
  included — changes to those do not require a layout re-run.

  This set is the canonical source of truth.  Keep it sorted for readability."
  #{:blocked
    :constraints-h
    :constraints-v
    :content           ;; path/text content can alter dimensions
    :fill-image        ;; image fill with stretch mode affects frame sizing
    :fixed-scroll
    :flip-x
    :flip-y
    :frame-id           ;; reparenting
    :grid-column
    :grid-row
    :height
    :layout             ;; adding/removing layout type on a container
    :layout-align-content
    :layout-align-items
    :layout-flex-dir
    :layout-gap
    :layout-grid-columns
    :layout-grid-rows
    :layout-grid-cells
    :layout-h-orientation
    :layout-item-absolute
    :layout-item-align-self
    :layout-item-h-sizing
    :layout-item-justify-self
    :layout-item-margin
    :layout-item-max-h
    :layout-item-max-w
    :layout-item-min-h
    :layout-item-min-w
    :layout-item-v-sizing
    :layout-justify-content
    :layout-justify-items
    :layout-padding
    :layout-wrap-type
    :parent-id          ;; reparenting
    :points
    :proportion
    :proportion-lock
    :rotate
    :rotation
    :selrect
    :shapes             ;; child order change counts as geometry
    :transform
    :transform-inverse
    :width
    :x
    :x1 :x2            ;; path points
    :y
    :y1 :y2})

(def ^:const geometry-change-types
  "Change-set `:type` values that always affect geometry, regardless of which
  attributes they touch.  `:mod-obj` is handled via `geometry-attrs` above."
  #{:add-obj
    :del-obj
    :mov-obj
    :add-page
    :del-page
    :mov-page})

;; ---------------------------------------------------------------------------
;; Predicate functions
;; ---------------------------------------------------------------------------

(defn geometry-affecting-operations?
  "Returns true if any individual operation within a `:mod-obj` change-set
  touches a geometry-affecting attribute."
  [operations]
  (boolean
   (some (fn [op]
           (case (:type op)
             :set    (contains? geometry-attrs (:attr op))
             :assign (boolean (some geometry-attrs (keys (:value op))))
             ;; :set-touched, :set-remote-synced — not geometry-affecting
             false))
         operations)))

(defn geometry-affecting-change?
  "Returns true when a single change-set requires the layout engine to
  re-run for affected shapes.

  Accepts the same map shape as entries in a ChangeSet vector, e.g.
    {:type :mod-obj :id <uuid> :operations [{:type :set :attr :x :val 10}]}"
  [{:keys [type operations]}]
  (or (contains? geometry-change-types type)
      (and (= type :mod-obj)
           (geometry-affecting-operations? operations))))

(defn geometry-affecting-changes?
  "Returns true when any change in the *coll* of change-sets requires the
  layout engine to re-run.  Short-circuits on the first match."
  [changes]
  (boolean (some geometry-affecting-change? changes)))
