;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.render-wasm.logos-layout
  "ClojureScript wrapper around the logos-layout-wasm JavaScript module.

  Exposes `init!`, `calc-flex-layout`, and `calc-grid-layout` for use
  from the Worker thread when the `logos-layout-wasm/v1` feature flag
  is enabled.

  The JS module (logos_layout.js) implements the C-ABI memory protocol:
    alloc_bytes → write JSON → call logos_calc_{flex,grid}_layout → read result."
  (:require ["./logos_layout.js" :as ll]))

;; ---------------------------------------------------------------------------
;; Lifecycle
;; ---------------------------------------------------------------------------

(defn init!
  "Load and compile the logos-layout-wasm WASM binary.
  Returns a JS Promise that resolves when the module is ready.
  Safe to call multiple times."
  ([]
   (init! "/js/logos_layout_wasm.wasm"))
  ([wasm-url]
   (.init ll wasm-url)))

(defn ready?
  "Returns true when the WASM module has been loaded."
  []
  (.isReady ll))

;; ---------------------------------------------------------------------------
;; Flex layout
;; ---------------------------------------------------------------------------

(defn calc-flex-layout
  "Compute flex layout via the Rust WASM engine.

  `container` map keys (all optional, snake_case strings or keywords):
    :container-width  :container-height
    :direction        (\"row\" | \"column\", default \"row\")
    :wrap             (\"no-wrap\" | \"wrap\", default \"no-wrap\")
    :justify-content  :align-items  :align-content  (default \"start\")
    :row-gap  :column-gap  :padding-top  :padding-right
    :padding-bottom  :padding-left

  `children` — seq of maps:
    :id (integer)  :width  :height
    :min-width  :max-width  :min-height  :max-height (optional)
    :h-sizing  :v-sizing  (\"fix\" | \"fill\" | \"auto\", default \"fix\")
    :align-self  (\"auto\" | \"start\" | \"end\" | \"center\" | \"stretch\")
    :absolute   (boolean)

  Returns a map:
    {:children [{:id … :x … :y … :width … :height …} …]}"
  [{:keys [container-width container-height
           direction wrap
           justify-content align-items align-content
           row-gap column-gap
           padding-top padding-right padding-bottom padding-left]
    :or {container-width 0 container-height 0
         direction "row" wrap "no-wrap"
         justify-content "start" align-items "start" align-content "start"
         row-gap 0 column-gap 0
         padding-top 0 padding-right 0 padding-bottom 0 padding-left 0}}
   children]
  (let [input #js {:container_width  container-width
                   :container_height container-height
                   :direction        direction
                   :wrap             wrap
                   :justify_content  justify-content
                   :align_items      align-items
                   :align_content    align-content
                   :row_gap          row-gap
                   :column_gap       column-gap
                   :padding_top      padding-top
                   :padding_right    padding-right
                   :padding_bottom   padding-bottom
                   :padding_left     padding-left
                   :children         (clj->js
                                      (mapv (fn [{:keys [id width height
                                                         min-width max-width
                                                         min-height max-height
                                                         h-sizing v-sizing
                                                         align-self absolute]
                                                  :or {h-sizing "fix" v-sizing "fix"
                                                       align-self "auto" absolute false}}]
                                              {:id         id
                                               :width      width
                                               :height     height
                                               :min_width  min-width
                                               :max_width  max-width
                                               :min_height min-height
                                               :max_height max-height
                                               :h_sizing   h-sizing
                                               :v_sizing   v-sizing
                                               :align_self align-self
                                               :absolute   absolute})
                                            children))}
        result (.flexLayout ll input)]
    (js->clj result :keywordize-keys true)))

;; ---------------------------------------------------------------------------
;; Grid layout
;; ---------------------------------------------------------------------------

(defn calc-grid-layout
  "Compute grid layout via the Rust WASM engine.

  `container` map keys:
    :container-width  :container-height
    :column-gap  :row-gap
    :padding-top  :padding-right  :padding-bottom  :padding-left
    :justify-items  :align-items  :justify-content  :align-content
    :direction  (\"row\" | \"column\")
    :columns  — vec of {:type \"fixed\"|\"flex\"|\"percent\"|\"auto\" :value n}
    :rows     — vec of {:type … :value …}
    :cells    — vec of {:shape-id n :row n :column n :row-span n :column-span n}

  `children` — seq of maps:
    :id  :min-width  :max-width  :min-height  :max-height

  Returns a map:
    {:resolved-columns […] :resolved-rows […]
     :children [{:id :x :y :width :height :col :row} …]}"
  [{:keys [container-width container-height
           column-gap row-gap
           padding-top padding-right padding-bottom padding-left
           justify-items align-items justify-content align-content
           direction columns rows cells]
    :or {container-width 0 container-height 0
         column-gap 0 row-gap 0
         padding-top 0 padding-right 0 padding-bottom 0 padding-left 0
         justify-items "start" align-items "start"
         justify-content "start" align-content "start"
         direction "row" columns [] rows [] cells []}}
   children]
  (let [track->js (fn [{:keys [type value] :or {type "fixed" value 0}}]
                    #js {:type type :value value})
        cell->js  (fn [{:keys [shape-id row column row-span column-span]
                        :or {row-span 1 column-span 1}}]
                    #js {:shape_id     shape-id
                         :row          row
                         :column       column
                         :row_span     row-span
                         :column_span  column-span})
        child->js (fn [{:keys [id min-width max-width min-height max-height]
                        :or {min-width 0 max-width 1e9
                             min-height 0 max-height 1e9}}]
                    #js {:id         id
                         :min_width  min-width
                         :max_width  max-width
                         :min_height min-height
                         :max_height max-height})
        input     #js {:container_width  container-width
                       :container_height container-height
                       :column_gap       column-gap
                       :row_gap          row-gap
                       :padding_top      padding-top
                       :padding_right    padding-right
                       :padding_bottom   padding-bottom
                       :padding_left     padding-left
                       :justify_items    justify-items
                       :align_items      align-items
                       :justify_content  justify-content
                       :align_content    align-content
                       :direction        direction
                       :columns          (clj->js (mapv track->js columns))
                       :rows             (clj->js (mapv track->js rows))
                       :cells            (clj->js (mapv cell->js cells))
                       :children         (clj->js (mapv child->js children))}
        result    (.gridLayout ll input)]
    (js->clj result :keywordize-keys true)))
