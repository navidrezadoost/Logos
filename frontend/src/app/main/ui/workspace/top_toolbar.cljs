;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.main.ui.workspace.top-toolbar
  (:require-macros [app.main.style :as stl])
  (:require
   [app.common.geom.point :as gpt]
   [app.main.data.event :as ev]
   [app.main.data.modal :as modal]
   [app.main.data.workspace :as dw]
   [app.main.data.workspace.common :as dwc]
   [app.main.data.workspace.media :as dwm]
   [app.main.data.workspace.shortcuts :as sc]
   [app.main.features :as features]
   [app.main.refs :as refs]
   [app.main.store :as st]
   [app.main.ui.components.file-uploader :refer [file-uploader*]]
   [app.main.ui.context :as ctx]
   [app.main.ui.ds.buttons.icon-button :refer [icon-button*]]
   [app.main.ui.ds.foundations.assets.icon :as i]
   [app.util.dom :as dom]
   [app.util.i18n :as i18n :refer [tr]]
   [app.util.timers :as ts]
   [okulary.core :as l]
   [potok.v2.core :as ptk]
   [rumext.v2 :as mf]))

(mf/defc image-upload*
  {::mf/wrap [mf/memo]}
  []
  (let [ref            (mf/use-ref nil)
        file-id        (mf/use-ctx ctx/current-file-id)

        on-click
        (mf/use-fn
         (fn []
           (st/emit! :interrupt (dw/clear-edition-mode))
           (dom/click (mf/ref-val ref))))

        on-selected
        (mf/use-fn
         (mf/deps file-id)
         (fn [blobs]
           ;; We don't want to add a ref because that redraws the component
           ;; for everychange. Better direct access on the callback.
           (let [vbox   (deref refs/vbox)
                 x      (+ (:x vbox) (/ (:width vbox) 2))
                 y      (+ (:y vbox) (/ (:height vbox) 2))
                 params {:file-id file-id
                         :blobs (seq blobs)
                         :position (gpt/point x y)}]
             (st/emit! (dwm/upload-media-workspace params)))))]
    [:li
     [:> icon-button* {:variant "ghost"
                       :class (stl/css :main-toolbar-options-button)
                       :icon i/img
                       :aria-label (tr "workspace.toolbar.image" (sc/get-tooltip :insert-image))
                       :tooltip-placement "bottom"
                       :on-click on-click}
      [:> file-uploader* {:input-id "image-upload"
                          :accept dwm/accept-image-types
                          :multi true
                          :ref ref
                          :on-selected on-selected}]]]))

(def ^:private toolbar-hidden-ref
  (l/derived (fn [state]
               (let [visibility      (get state :hide-toolbar)
                     path-edit-state (get state :edit-path)

                     selected        (get state :selected)
                     edition         (get state :edition)
                     single?         (= (count selected) 1)

                     path-editing?   (and single? (some? (get path-edit-state edition)))]
                 (if path-editing? true visibility)))
             refs/workspace-local))

(def ^:private toolbar-position-ref
  (l/derived #(get % :toolbar-position :bottom) refs/workspace-local))

(mf/defc top-toolbar*
  {::mf/memo true}
  [{:keys [layout]}]
  (let [drawtool      (mf/deref refs/selected-drawing-tool)
        edition       (mf/deref refs/selected-edition)

        profile       (mf/deref refs/profile)
        props         (get profile :props)

        read-only?    (mf/use-ctx ctx/workspace-read-only?)
        rulers?       (mf/deref refs/rulers?)
        hide-toolbar? (mf/deref toolbar-hidden-ref)
        toolbar-pos   (mf/deref toolbar-position-ref)

        show-pos-menu? (mf/use-state false)

        interrupt
        (mf/use-fn #(st/emit! :interrupt (dw/clear-edition-mode)))

        select-drawtool
        (mf/use-fn
         (fn [event]
           (let [tool (-> (dom/get-current-target event)
                          (dom/get-data "tool")
                          (keyword))]
             (st/emit! :interrupt (dw/clear-edition-mode))

             ;; Delay so anything that launched :interrupt can finish
             (ts/schedule 100 #(st/emit! (dw/select-for-drawing tool))))))

        toggle-debug-panel
        (mf/use-fn
         (mf/deps layout)
         (fn []
           (let [is-sidebar-closed? (contains? layout :collapse-left-sidebar)]
             (when is-sidebar-closed?
               (st/emit! (dw/toggle-layout-flag :collapse-left-sidebar)))
             (st/emit!
              (dw/remove-layout-flag :shortcuts)
              (-> (dw/toggle-layout-flag :debug-panel)
                  (vary-meta assoc ::ev/origin "workspace-left-toolbar"))))))

        toggle-toolbar
        (mf/use-fn
         (fn [event]
           (dom/blur! (dom/get-target event))
           (st/emit! (dwc/toggle-toolbar-visibility))))

        toggle-pos-menu
        (mf/use-fn
         (fn [event]
           (dom/stop-propagation event)
           (let [new-val (not @show-pos-menu?)]
             (reset! show-pos-menu? new-val)
             (when new-val
               (js/setTimeout
                #(js/document.addEventListener
                  "click"
                  (fn [_] (reset! show-pos-menu? false))
                  #js {:once true :capture false})
                0)))))

        set-position
        (mf/use-fn
         (fn [pos]
           (reset! show-pos-menu? false)
           (st/emit! (dwc/set-toolbar-position pos))))

        test-tooltip-board-text
        (if (not (:workspace-visited props))
          (tr "workspace.toolbar.frame-first-time" (sc/get-tooltip :draw-frame))
          (tr "workspace.toolbar.frame" (sc/get-tooltip :draw-frame)))]

    (when-not ^boolean read-only?
      [:aside {:class (stl/css-case :main-toolbar true
                                    :main-toolbar-no-rulers (not rulers?)
                                    :main-toolbar-hidden   hide-toolbar?
                                    :main-toolbar-vertical (or (= toolbar-pos :left)
                                                               (= toolbar-pos :right)))
               :style (case toolbar-pos
                        :top    #js {:top "28px"  :left "50%"  :transform "translateX(-50%)" :bottom "unset" :right "unset"}
                        :left   #js {:top "50%"   :left "28px" :transform "translateY(-50%)" :bottom "unset" :right "unset"}
                        :right  #js {:top "50%"   :right "28px" :left "unset" :transform "translateY(-50%)" :bottom "unset"}
                        #js {:bottom "28px" :left "50%" :transform "translateX(-50%)" :top "unset" :right "unset"})}
       [:ul {:class (stl/css :main-toolbar-options)
             :data-testid "toolbar-options"}
        [:li
         [:> icon-button* {:variant "ghost"
                           :class (stl/css :main-toolbar-options-button)
                           :icon i/move
                           :aria-pressed (and (nil? drawtool) (not edition))
                           :aria-label (tr "workspace.toolbar.move" (sc/get-tooltip :move))
                           :tooltip-placement "bottom"
                           :on-click interrupt}]]
        [:*
         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/board
                            :aria-pressed (= drawtool :frame)
                            :aria-label test-tooltip-board-text
                            :tooltip-placement "bottom"
                            :on-click select-drawtool
                            :data-tool "frame"
                            :data-testid "artboard-btn"}]]
         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/rectangle
                            :aria-pressed (= drawtool :rect)
                            :aria-label (tr "workspace.toolbar.rect" (sc/get-tooltip :draw-rect))
                            :tooltip-placement "bottom"
                            :on-click select-drawtool
                            :data-tool "rect"
                            :data-testid "rect-btn"}]]
         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/elipse
                            :aria-pressed (= drawtool :circle)
                            :aria-label (tr "workspace.toolbar.ellipse" (sc/get-tooltip :draw-ellipse))
                            :tooltip-placement "bottom"
                            :on-click select-drawtool
                            :data-tool "circle"
                            :data-testid "ellipse-btn"}]]
         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/text
                            :aria-pressed (= drawtool :text)
                            :aria-label (tr "workspace.toolbar.text" (sc/get-tooltip :draw-text))
                            :tooltip-placement "bottom"
                            :on-click select-drawtool
                            :data-tool "text"
                            :data-testid "text-btn"}]]

         [:> image-upload*]

         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/curve
                            :aria-pressed (= drawtool :curve)
                            :aria-label (tr "workspace.toolbar.curve" (sc/get-tooltip :draw-curve))
                            :tooltip-placement "bottom"
                            :on-click select-drawtool
                            :data-tool "curve"
                            :data-testid "curve-btn"}]]
         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/path
                            :aria-pressed (= drawtool :path)
                            :aria-label (tr "workspace.toolbar.path" (sc/get-tooltip :draw-path))
                            :tooltip-placement "bottom"
                            :on-click select-drawtool
                            :data-tool "path"
                            :data-testid "path-btn"}]]

         (when (features/active-feature? @st/state "plugins/runtime")
           [:li
            [:> icon-button* {:variant "ghost"
                              :class (stl/css :main-toolbar-options-button)
                              :icon i/puzzle
                              :aria-label (tr "workspace.toolbar.plugins" (sc/get-tooltip :plugins))
                              :tooltip-placement "bottom"
                              :on-click #(st/emit!
                                          (ptk/data-event ::ev/event {::ev/name "open-plugins-manager"
                                                                      ::ev/origin "workspace:toolbar"})
                                          (modal/show :plugin-management {}))
                              :data-tool "plugins"
                              :data-testid "plugins-btn"}]])

         (when *assert*
           [:li
            [:> icon-button* {:variant "ghost"
                              :class (stl/css :main-toolbar-options-button)
                              :icon i/bug
                              :aria-pressed (contains? layout :debug-panel)
                              :aria-label (tr "workspace.toolbar.debug")
                              :tooltip-placement "bottom"
                              :on-click toggle-debug-panel}]])

         ;; ── Position trigger button (popup is at [:aside] level) ──
         [:li
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/expand
                            :aria-pressed @show-pos-menu?
                            :aria-label "Toolbar position"
                            :tooltip-placement "bottom"
                            :on-click toggle-pos-menu}]]]]

       ;; ── Position popup – direct child of [:aside] ──────────────
       ;; Shows only the 3 positions that are NOT the current one.
       (when @show-pos-menu?
         (let [all-positions [[:top    i/arrow-up    "Top"]
                              [:left   i/arrow-left  "Left"]
                              [:right  i/arrow-right "Right"]
                              [:bottom i/arrow-down  "Bottom"]]
               options       (remove #(= (first %) toolbar-pos) all-positions)]
           [:div {:class (stl/css :toolbar-position-menu)
                  :on-click dom/stop-propagation
                  :style (case toolbar-pos
                           :top   #js {:top "calc(100% + 8px)"   :left "50%" :transform "translateX(-50%)" :bottom "unset" :right "unset"}
                           :left  #js {:left "calc(100% + 8px)"  :top "50%"  :transform "translateY(-50%)" :bottom "unset" :right "unset"}
                           :right #js {:right "calc(100% + 8px)" :top "50%"  :transform "translateY(-50%)" :bottom "unset" :left "unset"}
                           #js {:bottom "calc(100% + 8px)" :left "50%" :transform "translateX(-50%)" :top "unset" :right "unset"})}
            (for [[pos icon label] options]
              [:div {:key (name pos) :class (stl/css :pos-row)}
               [:> icon-button* {:variant "ghost"
                                 :class (stl/css :pos-btn)
                                 :icon icon
                                 :aria-label label
                                 :on-click #(set-position pos)}]
               [:span {:class (stl/css :pos-label)} label]])]))

       [:button {:title (tr "workspace.toolbar.toggle-toolbar")
                 :aria-label (tr "workspace.toolbar.toggle-toolbar")
                 :class (stl/css :toolbar-handler)
                 :on-click toggle-toolbar}
        [:div {:class (stl/css :toolbar-handler-btn)}]]])))
