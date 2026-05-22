;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.main.data.workspace.drawing.shapes
  "Drawing handlers for parametric shapes: line, arrow, polygon, star.
   Each uses the same box-drag interaction as rect/circle but finalizes
   the object as a :path shape with computed path content."
  (:require
   [app.common.data.macros :as dm]
   [app.common.geom.point :as gpt]
   [app.common.geom.rect :as grc]
   [app.common.geom.shapes.flex-layout :as gslf]
   [app.common.geom.shapes.grid-layout :as gslg]
   [app.common.math :as mth]
   [app.common.types.container :as ctn]
   [app.common.types.path.segment :as path.segment]
   [app.common.types.shape :as cts]
   [app.common.types.shape-tree :as ctst]
   [app.common.types.shape.layout :as ctl]
   [app.main.data.helpers :as dsh]
   [app.main.data.workspace.drawing.box :as box]
   [app.main.data.workspace.drawing.common :as common]
   [app.main.snap :as snap]
   [app.main.streams :as ms]
   [app.util.array :as array]
   [app.util.mouse :as mse]
   [beicon.v2.core :as rx]
   [potok.v2.core :as ptk]))

;;; ── Path content generators ──────────────────────────────────────────

(defn- line-content-from-points
  "A straight line between two explicit points (preserves drag direction)."
  [p0 p1]
  (path.segment/points->content [p0 p1]))

(defn- rect->line-content
  "Fallback: diagonal line from bbox top-left to bottom-right.
   Prefer line-content-from-points when draw-start/draw-end are available."
  [{:keys [x y width height]}]
  (path.segment/points->content
   [(gpt/point x y)
    (gpt/point (+ x width) (+ y height))]))

(defn- arrow-content-from-points
  "Arrow from explicit start point to explicit end point (preserves drag direction)."
  [p0 p1]
  (let [sx (:x p0) sy (:y p0)
        ex (:x p1) ey (:y p1)
        dx (- ex sx)   dy (- ey sy)
        len (mth/sqrt (+ (* dx dx) (* dy dy)))
        safe (max len 1)
        ux (/ dx safe) uy (/ dy safe)
        hs (min 16 (* safe 0.3))
        ax1 (- ex (* hs (+ (* ux 0.866) (* uy -0.5))))
        ay1 (- ey (* hs (+ (* uy 0.866) (* ux 0.5))))
        ax2 (- ex (* hs (+ (* ux 0.866) (* uy 0.5))))
        ay2 (- ey (* hs (+ (* uy 0.866) (* ux -0.5))))]
    (path.segment/points->content
     [(gpt/point sx sy)
      (gpt/point ex ey)
      (gpt/point ax1 ay1)
      (gpt/point ex ey)
      (gpt/point ax2 ay2)])))

(defn- rect->arrow-content
  "Fallback: arrow from bbox top-left to bottom-right.
   Prefer arrow-content-from-points when draw-start/draw-end are available."
  [{:keys [x y width height]}]
  (let [sx x  sy y
        ex (+ x width) ey (+ y height)
        dx (- ex sx)   dy (- ey sy)
        len (mth/sqrt (+ (* dx dx) (* dy dy)))
        safe (max len 1)
        ux (/ dx safe) uy (/ dy safe)
        hs (min 16 (* safe 0.3))
        ;; Two arrowhead wing tips (rotate ±30° from the back-direction)
        ax1 (- ex (* hs (+ (* ux 0.866) (* uy -0.5))))
        ay1 (- ey (* hs (+ (* uy 0.866) (* ux 0.5))))
        ax2 (- ex (* hs (+ (* ux 0.866) (* uy 0.5))))
        ay2 (- ey (* hs (+ (* uy 0.866) (* ux -0.5))))]
    ;; Continuous path: shaft + arrowhead V, no separate subpaths needed
    (path.segment/points->content
     [(gpt/point sx sy)
      (gpt/point ex ey)
      (gpt/point ax1 ay1)
      (gpt/point ex ey)
      (gpt/point ax2 ay2)])))

(defn- rect->polygon-content
  "Regular N-sided polygon inscribed in the bounding box."
  [{:keys [x y width height]} sides]
  (let [cx (+ x (/ width 2))
        cy (+ y (/ height 2))
        rx (/ width 2)
        ry (/ height 2)
        pts (mapv (fn [i]
                    (let [angle (- (* 2 mth/PI (/ i sides)) (/ mth/PI 2))]
                      (gpt/point (+ cx (* rx (mth/cos angle)))
                                 (+ cy (* ry (mth/sin angle))))))
                  (range sides))]
    (path.segment/points->content pts :close true)))

(defn- rect->star-content
  "5-pointed star inscribed in the bounding box."
  [{:keys [x y width height]} n]
  (let [cx   (+ x (/ width 2))
        cy   (+ y (/ height 2))
        orx  (/ width 2)
        ory  (/ height 2)
        irx  (* orx 0.4)
        iry  (* ory 0.4)
        total (* 2 n)
        pts  (mapv (fn [i]
                     (let [angle (- (* 2 mth/PI (/ i total)) (/ mth/PI 2))
                           rx    (if (even? i) orx irx)
                           ry    (if (even? i) ory iry)]
                       (gpt/point (+ cx (* rx (mth/cos angle)))
                                  (+ cy (* ry (mth/sin angle))))))
                   (range total))]
    (path.segment/points->content pts :close true)))

(defn- shape-content
  ([shape-type selrect]
   (shape-content shape-type selrect nil nil))
  ([shape-type selrect draw-start draw-end]
   (case shape-type
     :line    (if (and draw-start draw-end)
                (line-content-from-points draw-start draw-end)
                (rect->line-content selrect))
     :arrow   (if (and draw-start draw-end)
                (arrow-content-from-points draw-start draw-end)
                (rect->arrow-content selrect))
     :polygon (rect->polygon-content selrect 5)
     :star    (rect->star-content   selrect 5))))

;;; ── Finalize step ──────────────────────────────────────────────────

(defn- finish-shape-drawing
  "Converts the box-dragged :rect into a :path with computed content.
   For click-only draws (no drag), uses a 100×100 default bounding box.
   For :line and :arrow uses stored draw-start/draw-end so the drag
   direction is preserved (normalized selrect loses that information)."
  [shape-type]
  (ptk/reify ::finish-shape-drawing
    ptk/UpdateEvent
    (update [_ state]
      (let [drawing    (get state :workspace-drawing)
            draw-start (get drawing :draw-start)
            draw-end   (get drawing :draw-end)]
        (update-in state [:workspace-drawing :object]
                   (fn [{:keys [selrect click-draw? x y] :as shape}]
                     ;; For a click-only draw the bounding rect is degenerate.
                     ;; Use a proper 100×100 box instead so the shape is visible.
                     (let [box     (if (or click-draw?
                                          (nil? selrect)
                                          (< (:width  selrect 0) 1)
                                          (< (:height selrect 0) 1))
                                     {:x (or x (:x selrect 0))
                                      :y (or y (:y selrect 0))
                                      :width  100
                                      :height 100}
                                     selrect)
                           ;; Only use directional points when the user actually dragged.
                           ;; click-draw? means no drag occurred → draw-start = draw-end,
                           ;; which would produce a zero-length path, so fall back to the
                           ;; bounding-box generator.
                           content (if (or click-draw? (nil? draw-start) (nil? draw-end))
                                     (shape-content shape-type box)
                                     (shape-content shape-type box draw-start draw-end))
                           sr      (path.segment/content->selrect content)
                           pts     (when sr (grc/rect->points sr))]
                       (-> shape
                           (assoc :type        :path)
                           (assoc :content     content)
                           (assoc :click-draw? false)
                           (cond-> (some? sr)  (assoc :selrect sr))
                           (cond-> (some? pts) (assoc :points  pts)))))))))

;;; ── Main entry point ───────────────────────────────────────────────

(defn handle-drawing
  "Handle drawing for parametric path shapes (line, arrow, polygon, star).
   Uses the same box-drag interaction as box/handle-drawing but finalizes
   the object as a :path shape."
  [shape-type]
  (ptk/reify ::handle-drawing
    ptk/WatchEvent
    (watch [_ state stream]
      (js/console.log "[SHAPES] handle-drawing WatchEvent called" (str "shape-type=" shape-type))
      (let [stopper      (mse/drag-stopper stream)
            layout       (get state :workspace-layout)
            zoom         (dm/get-in state [:workspace-local :zoom] 1)
            snap-pixel?  (contains? layout :snap-pixel-grid)
            initial      (cond-> @ms/mouse-position snap-pixel? (gpt/round-step 1))

            page-id      (:current-page-id state)
            objects      (dsh/lookup-page-objects state page-id)
            focus        (:workspace-focus-selected state)

            fid          (->> (ctst/top-nested-frame objects initial)
                              (ctn/get-first-valid-parent objects)
                              :id)

            flex-layout? (ctl/flex-layout? objects fid)
            grid-layout? (ctl/grid-layout? objects fid)

            drop-index   (when flex-layout? (gslf/get-drop-index fid objects initial))
            drop-cell    (when grid-layout? (gslg/get-drop-cell fid objects initial))

            ;; Use :rect type for box-drag interaction; we convert to :path on finish
            shape        (-> (cts/setup-shape {:type         :rect
                                               :x            (:x initial)
                                               :y            (:y initial)
                                               :frame-id     fid
                                               :parent-id    fid
                                               :initialized? true
                                               :click-draw?  true})
                             (cond-> (some? drop-index) (with-meta {:index drop-index}))
                             (cond-> (some? drop-cell)  (with-meta {:cell  drop-cell})))]

        (rx/concat
         ;; Put initial shape into drawing state; record draw-start/draw-end for
         ;; direction-preserving line/arrow content generation.
         (rx/of (fn [s]
                  (-> s
                      (update :workspace-drawing assoc :object shape)
                      (assoc-in [:workspace-drawing :draw-start] initial)
                      (assoc-in [:workspace-drawing :draw-end]   initial))))
         ;; Snap + drag tracking (reuse box helpers)
         (->> (rx/concat
               (->> (snap/closest-snap-point page-id [shape] objects layout zoom focus initial)
                    (rx/map box/move-drawing))
               (->> ms/mouse-position
                    (rx/filter #(> (gpt/distance % initial) (/ 2 zoom)))
                    (rx/take-until stopper)
                    (rx/with-latest-from ms/mouse-position-shift ms/mouse-position-mod)
                    (rx/switch-map
                     (fn [[point :as current]]
                       (->> (snap/closest-snap-point page-id [shape] objects layout zoom focus point)
                            (rx/map (partial array/conj current)))))
                    (rx/map
                     (fn [[_ shift? mod? point]]
                       (let [snapped (cond-> point snap-pixel? (gpt/round-step 1))]
                         (fn [s]
                           (-> s
                               (box/update-drawing initial snapped shift? mod?)
                               (assoc-in [:workspace-drawing :draw-end] snapped))))))))
              (rx/take-until stopper))
         ;; Convert to :path and commit
         (->> (rx/of (finish-shape-drawing shape-type)
                     (common/handle-finish-drawing))
              (rx/delay 100)))))))
