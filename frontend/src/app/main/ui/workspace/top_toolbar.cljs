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
   [app.main.ui.ds.foundations.assets.icon :as i :refer [icon*]]
   [app.util.dom :as dom]
   [app.util.i18n :as i18n :refer [tr]]
   [app.util.storage :as storage]
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
  (l/derived (fn [local]
               (or (get local :toolbar-position)
                   (some-> (get @storage/user :app.main.data.workspace/toolbar-position) keyword)
                   :bottom))
             refs/workspace-local))

;; ─── Tool-group definitions ──────────────────────────────────────────────────
;; Each group has a :default tool (shown when none of the group is active),
;; a :tools vector of {:tool kw :label str :shortcut str :icon icon-id-or-nil
;;                     :text str-or-nil} entries.
;; :icon is a ds icon-id var; :text is a fallback when no icon exists.

(def ^:private tool-groups
  [{:id      :move-group
    :tools   [{:tool :move   :label "Move"      :shortcut "V"       :icon i/move      :text nil}
              {:tool :hand   :label "Hand"       :shortcut "H"       :icon nil         :text "✋"}
              {:tool :scale  :label "Scale"      :shortcut "K"       :icon nil         :text "⤡"}]}
   {:id      :frame-group
    :tools   [{:tool :frame  :label "Frame"      :shortcut "F"       :icon i/board     :text nil}
              {:tool :slice  :label "Slice"      :shortcut "S"       :icon i/stroke-rectangle :text nil}]}
   {:id      :shape-group
    :tools   [{:tool :rect    :label "Rectangle" :shortcut "R" :icon i/rectangle       :text nil}
              {:tool :circle  :label "Ellipse"   :shortcut "O" :icon i/elipse          :text nil}
              {:tool :line    :label "Line"       :shortcut "L" :icon i/stroke-arrow    :text nil}
              {:tool :arrow   :label "Arrow"      :shortcut nil :icon i/arrow           :text nil}
              {:tool :polygon :label "Polygon"    :shortcut nil :icon i/stroke-triangle :text nil}
              {:tool :star    :label "Star"        :shortcut nil :icon nil              :text "☆"}]}
   {:id      :pen-group
    :tools   [{:tool :path   :label "Pen"    :shortcut "P" :icon i/path  :text nil}
              {:tool :curve  :label "Pencil" :shortcut ""  :icon i/curve :text nil}]}])

(defn- tool-in-group?
  "True if `tool` belongs to the given group entry."
  [group tool]
  (some #(= (:tool %) tool) (:tools group)))

(defn- active-tool-def
  "Returns the tool-def that should be displayed for a group given the current
  drawtool / move-active state and the group's locally-remembered last tool."
  [group drawtool move-active? last-tool]
  (let [tools      (:tools group)
        active-def (or (some #(when (if (= (:tool %) :move)
                                     move-active?
                                     (= drawtool (:tool %)))
                                 %)
                             tools)
                       (some #(when (= (:tool %) last-tool) %) tools)
                       (first tools))]
    active-def))

;; ─── Grouped tool button ──────────────────────────────────────────────────────

(mf/defc tool-group-button*
  "Renders one grouped tool button with a small ▾ chevron that opens a dropdown.
   Props:
     :group       – the group map (from tool-groups)
     :drawtool     – currently selected drawing tool keyword, or nil
     :move-active  – true when no draw tool is active (Move is selected)
     :open?        – whether this group's dropdown is open
     :last-tool    – last tool selected in this group (atom-deref value)
     :pending-tool – tool clicked but not yet in global state (immediate feedback)
     :on-activate  – fn called with a tool keyword to activate a tool
     :on-open      – fn called with no args to open this group's dropdown
     :on-close     – fn called to close the dropdown"
  [{:keys [group drawtool move-active? open? last-tool pending-tool on-activate on-close on-open]}]
  (let [;; Prefer pending-tool for icon display (immediate feedback)
        display-def  (active-tool-def group drawtool move-active?
                                      (or pending-tool last-tool))
        ;; Group is active if drawtool or pending-tool belongs to this group.
        ;; Also suppress move-group highlight when a non-move tool is pending.
        group-active (or (and (= (:id group) :move-group)
                              move-active?
                              (nil? pending-tool))
                        (tool-in-group? group drawtool)
                        (and (some? pending-tool)
                             (tool-in-group? group pending-tool)))
        btn-ref      (mf/use-ref nil)
        btn-rect     (mf/use-state nil)]

    ;; No document-listener needed — outside clicks are caught by the backdrop
    ;; rendered inside the portal (see below).

    [:li {:ref btn-ref
          :style #js {:position "relative" :display "flex" :alignItems "center"}}
     ;; ── Main tool button ────────────────────────────────────────────────
     [:> icon-button* {:variant        "ghost"
                       :class          (stl/css :main-toolbar-options-button)
                       :icon           (or (:icon display-def) i/move)
                       :aria-pressed   group-active
                       :aria-label     (str (:label display-def)
                                            (when-let [s (:shortcut display-def)]
                                              (str " (" s ")")))
                       :tooltip-placement "bottom"
                       :on-click       (fn [_] (on-activate (:tool display-def)))}
      ;; Text fallback overlay when no ds icon exists
      (when (nil? (:icon display-def))
        [:span {:style #js {:position "absolute" :top "50%" :left "50%"
                             :transform "translate(-50%,-50%)"
                             :fontSize "13px" :pointerEvents "none"}}
         (:text display-def)])]

     ;; ── Chevron (dropdown trigger) ───────────────────────────────────────
     [:button {:title          (str "More " (name (:id group)) " tools")
               :style          #js {:position  "absolute"
                                     :bottom    "2px"
                                     :right     "2px"
                                     :width     "14px"
                                     :height    "14px"
                                     :padding   "0"
                                     :border    "none"
                                     :background "transparent"
                                     :cursor    "pointer"
                                     :color     (if group-active "#cba6f7" "#6c7086")
                                     :fontSize  "8px"
                                     :lineHeight "1"
                                     :display   "flex"
                                     :alignItems "center"
                                     :justifyContent "center"
                                     :zIndex    "1"}
               ;; Stop BOTH pointerdown and click so nothing bubbles to the
               ;; workspace viewport handlers that auto-hide the toolbar.
               :on-pointer-down (fn [e]
                                   (dom/stop-propagation e)
                                   (dom/prevent-default e))
               :on-click        (fn [e]
                                   (dom/stop-propagation e)
                                   (dom/prevent-default e)
                                   (let [el (mf/ref-val btn-ref)]
                                     (when el
                                       (let [r (.getBoundingClientRect el)]
                                         (reset! btn-rect {:top    (.-top r)
                                                           :left   (.-left r)
                                                           :right  (.-right r)
                                                           :bottom (.-bottom r)
                                                           :width  (.-width r)
                                                           :height (.-height r)}))))
                                   (on-open))}
      "▾"]

     ;; ── Dropdown ─────────────────────────────────────────────────────────
     (when (and open? @btn-rect)
       (let [{:keys [top left right bottom width height]} @btn-rect
             vp-w  js/window.innerWidth
             vp-h  js/window.innerHeight
             below?      (< (+ bottom 8 (* 36 (count (:tools group)))) vp-h)
             right?      (< (+ left 220) vp-w)
             popup-top   (when below?       (+ bottom 6))
             popup-bot   (when (not below?) (+ (- vp-h top) 6))
             popup-left  (when right?       left)
             popup-right (when (not right?) (+ (- vp-w right) (/ width 2)))]
         (mf/portal
          (mf/html
           [:*
            ;; Full-screen backdrop — catches outside clicks without swallowing them
            ;; from inside items (backdrop z-index is below the popup).
            [:div {:style    #js {:position "fixed" :top 0 :left 0
                                  :right 0 :bottom 0 :zIndex 9997}
                   :on-click (fn [e] (dom/stop-propagation e) (on-close))}]
            ;; Popup menu
            [:div {:style     (clj->js (cond-> {:position      "fixed"
                                                :zIndex        9998
                                                :background    "#18181a"
                                                :border        "1px solid #313244"
                                                :borderRadius  "8px"
                                                :boxShadow     "0 8px 24px rgba(0,0,0,.5)"
                                                :padding       "4px 0"
                                                :minWidth      "180px"
                                                :display       "flex"
                                                :flexDirection "column"}
                                         popup-top   (assoc :top    (str popup-top   "px"))
                                         popup-bot   (assoc :bottom (str popup-bot   "px"))
                                         popup-left  (assoc :left   (str popup-left  "px"))
                                         popup-right (assoc :right  (str popup-right "px"))))
                   :on-click dom/stop-propagation}
             (for [{:keys [tool label shortcut icon text]} (:tools group)]
               (let [is-active (if (= tool :move)
                                 (and move-active? (nil? pending-tool))
                                 (or (= drawtool tool) (= pending-tool tool)))]
                 [:button {:key      (name tool)
                           :style    #js {:display       "flex"
                                          :flexDirection "row"
                                          :alignItems    "center"
                                          :gap           "8px"
                                          :padding       "0 12px"
                                          :height        "36px"
                                          :border        "none"
                                          :cursor        "pointer"
                                          :width         "100%"
                                          :textAlign     "left"
                                          :background    (if is-active "rgba(203,166,247,.15)" "transparent")
                                          :color         (if is-active "#cba6f7" "#cdd6f4")}
                           :on-click (fn [_] (on-activate tool) (on-close))}
                  [:span {:style #js {:width "20px" :textAlign "center" :flexShrink "0"}}
                   (if icon
                     [:> icon* {:icon-id icon
                                :style   #js {:width "14px" :height "14px"
                                              :color (if is-active "#cba6f7" "#cdd6f4")}}]
                     [:span {:style #js {:fontSize "13px"}} text])]
                  [:span {:style #js {:flex "1" :fontSize "13px"}} label]
                  (when (seq shortcut)
                    [:span {:style #js {:fontSize "11px" :color "#585b70" :fontFamily "monospace"}} shortcut])
                  (when is-active
                    [:span {:style #js {:fontSize "8px" :color "#cba6f7" :marginLeft "2px"}} "●"])]))]])
          (dom/get-body))))]))
;; ─── Main component ───────────────────────────────────────────────────────────

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

        ;; Which group's dropdown is open (nil = none)
        open-group     (mf/use-state nil)
        ;; Last-used tool per group (so the displayed icon "sticks" after selection)
        last-in-group  (mf/use-state {:move-group :move :frame-group :frame :shape-group :rect :pen-group :path})
        ;; Tool just clicked but not yet committed to global state.
        ;; Gives immediate visual feedback while waiting for the async dispatch.
        pending-tool   (mf/use-state nil)

        show-pos-menu? (mf/use-state false)
        pos-btn-ref    (mf/use-ref nil)
        pos-rect       (mf/use-state nil)

        move-active?   (and (nil? drawtool) (not edition))

        interrupt
        (mf/use-fn #(st/emit! :interrupt (dw/clear-edition-mode)))

        ;; Activate a tool — handles :move specially (interrupt, not draw)
        activate-tool
        (mf/use-fn
         (mf/deps drawtool edition)
         (fn [tool]
           (js/console.log "[TOOLBAR] activate-tool called" (str "tool=" tool) (str "current-drawtool=" drawtool))
           ;; Update last-used per group immediately (icon shows right away)
           (doseq [g tool-groups]
             (when (tool-in-group? g tool)
               (swap! last-in-group assoc (:id g) tool)))
           (if (= tool :move)
             (do
               (reset! pending-tool nil)
               (st/emit! :interrupt (dw/clear-edition-mode)))
             (do
               (reset! pending-tool tool)
               (st/emit! :interrupt (dw/clear-edition-mode))
               (ts/schedule 100 (fn []
                 (js/console.log "[TOOLBAR] schedule-100 firing: emitting select-for-drawing" (str tool))
                 (reset! pending-tool nil)
                 ;; Emit :interrupt before select-for-drawing to cancel any stale
                 ;; re-arm subscriptions left over from previous rapid tool switches.
                 (st/emit! :interrupt (dw/select-for-drawing tool))))))))

        select-drawtool
        (mf/use-fn
         (fn [event]
           (let [tool (-> (dom/get-current-target event)
                          (dom/get-data "tool")
                          (keyword))]
             (st/emit! :interrupt (dw/clear-edition-mode))
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
               (when-let [el (mf/ref-val pos-btn-ref)]
                 (let [r (.getBoundingClientRect el)]
                   (reset! pos-rect {:top    (.-top r)
                                     :left   (.-left r)
                                     :right  (.-right r)
                                     :bottom (.-bottom r)
                                     :width  (.-width r)
                                     :height (.-height r)})))
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
           (st/emit! (dwc/set-toolbar-position pos))))]

    (when-not ^boolean read-only?
      [:aside {:class (stl/css-case :main-toolbar true
                                    :main-toolbar-no-rulers (not rulers?)
                                    :main-toolbar-hidden   hide-toolbar?
                                    :main-toolbar-vertical (or (= toolbar-pos :left)
                                                               (= toolbar-pos :right)))
               :style (case toolbar-pos
                        :top    #js {:top "28px"  :left "50%"  :transform "translateX(-50%)" :bottom "unset" :right "unset"  :flexDirection "row"  :height "56px" :width "auto"}
                        :left   #js {:top "50%"   :left "28px" :transform "translateY(-50%)" :bottom "unset" :right "unset"  :flexDirection "column" :height "auto" :width "56px"}
                        :right  #js {:top "50%"   :right "28px" :left "unset" :transform "translateY(-50%)" :bottom "unset" :flexDirection "column" :height "auto" :width "56px"}
                        #js {:bottom "28px" :left "50%" :transform "translateX(-50%)" :top "unset" :right "unset" :flexDirection "row" :height "56px" :width "auto"})}
       [:ul {:class (stl/css :main-toolbar-options)
             :data-testid "toolbar-options"
             :style (when (or (= toolbar-pos :left) (= toolbar-pos :right))
                      #js {:flexDirection "column" :alignItems "center"})}

        ;; ── Grouped tool buttons ──────────────────────────────────────────
        (for [group tool-groups]
          [:> tool-group-button*
           {:key         (name (:id group))
            :group       group
            :drawtool    drawtool
            :move-active? move-active?
            :open?       (= @open-group (:id group))
            :last-tool   (get @last-in-group (:id group))
            :pending-tool @pending-tool
            :on-activate activate-tool
            :on-open     (fn [] (reset! open-group (:id group)))
            :on-close    (fn [] (reset! open-group nil))}])

        ;; ── Standalone: text ─────────────────────────────────────────────
        [:*
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

         ;; ── Position trigger button ───────────────────────────────────
         [:li {:ref pos-btn-ref}
          [:> icon-button* {:variant "ghost"
                            :class (stl/css :main-toolbar-options-button)
                            :icon i/expand
                            :aria-pressed @show-pos-menu?
                            :aria-label "Toolbar position"
                            :tooltip-placement "bottom"
                            :on-click toggle-pos-menu}]]]]

       [:button {:title (tr "workspace.toolbar.toggle-toolbar")
                 :aria-label (tr "workspace.toolbar.toggle-toolbar")
                 :class (stl/css :toolbar-handler)
                 :on-click toggle-toolbar}
        [:div {:class (stl/css :toolbar-handler-btn)}]]

       ;; ── Position popup ────────────────────────────────────────────────
       (when (and @show-pos-menu? @pos-rect)
         (let [{:keys [top left right bottom width height]} @pos-rect
               vw  js/window.innerWidth
               gap 6
               all-positions [[:top    i/arrow-up    "Top"]
                               [:left   i/arrow-left  "Left"]
                               [:right  i/arrow-right "Right"]
                               [:bottom i/arrow-down  "Bottom"]]
               options    (remove #(= (first %) toolbar-pos) all-positions)
               popup-style (case toolbar-pos
                             :top    {:top    (str (+ bottom gap) "px")
                                      :left   (str (+ left (/ width 2)) "px")
                                      :transform "translateX(-50%)"}
                             :left   {:top    (str (+ top (/ height 2)) "px")
                                      :left   (str (+ right gap) "px")
                                      :transform "translateY(-50%)"}
                             :right  {:top    (str (+ top (/ height 2)) "px")
                                      :right  (str (+ (- vw left) gap) "px")
                                      :transform "translateY(-50%)"}
                             {:bottom (str (+ (- js/window.innerHeight top) gap) "px")
                              :left   (str (+ left (/ width 2)) "px")
                              :transform "translateX(-50%)"})]
           (mf/portal
            (mf/html
             [:div {:class (stl/css :toolbar-position-menu)
                    :on-click dom/stop-propagation
                    :style (clj->js (assoc popup-style
                                           :position "fixed"
                                           :zIndex 9999
                                           :backgroundColor "#18181a"
                                           :color "#fff"
                                           :border "2px solid #404040"
                                           :borderRadius "8px"
                                           :padding "4px"
                                           :minWidth "120px"
                                           :display "flex"
                                           :flexDirection "column"
                                           :boxShadow "0 8px 24px rgba(0,0,0,0.5)"))}
              (for [[pos icon label] options]
                [:button {:key      (name pos)
                          :class    (stl/css :pos-row)
                          :style    #js {:display "flex" :flexDirection "row" :alignItems "center"
                                         :gap "8px" :height "36px" :padding "0 8px"
                                         :border "none" :cursor "pointer" :width "100%"
                                         :backgroundColor "transparent" :color "#fff"
                                         :borderRadius "4px"}
                          :on-click (fn [e]
                                      (dom/stop-propagation e)
                                      (set-position pos))}
                 [:> icon* {:icon-id icon :class (stl/css :pos-icon) :style #js {:width "16px" :height "16px" :color "#fff"}}]
                 [:span {:class (stl/css :pos-label) :style #js {:fontSize "11px" :color "#fff"}} label]])])
            (dom/get-body))))])))
