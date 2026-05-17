;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0.
;;
;; P1.7 — Benchmark file generator.
;;
;; Produces `fixtures/large-canvas.penpot` — a v3-format ZIP containing
;; 10,000+ shapes across 5 pages. Run from test/benchmarks/:
;;
;;   clojure -M:gen-benchmark [--output fixtures/large-canvas.penpot]
;;
;; The file exercises:
;;   Page 1  "Dense Frames"  — 3 frames × 700 rects + ellipses  = 2100 shapes
;;   Page 2  "Stroke Paths"  — 2000 paths with dashed strokes
;;   Page 3  "Typography"    — 2000 text/placeholder shapes
;;   Page 4  "Nested Groups" — deeply nested groups, ~2000 shapes total
;;   Page 5  "Grid Layout"   — flex-frame with 2000 leaf rects
;;
;; Total ≥ 10,100 shapes.
;;
;; The generator intentionally avoids embedded media (images) so it can run
;; without a database or storage service. Shape data is constructed as explicit
;; Clojure maps that match the JSON schema consumed by the v3 importer —
;; the same schema that Malli generators would produce.

(ns benchmarks.generate-file
  (:require
   [clojure.data.json :as json]
   [clojure.java.io :as io])
  (:import
   [java.io File FileOutputStream]
   [java.util UUID]
   [java.util.zip ZipEntry ZipOutputStream])
  (:gen-class))

;; ---------------------------------------------------------------------------
;; Helper — write a Clojure map (or vector) as a JSON ZipEntry.
;; ---------------------------------------------------------------------------

(defn- write-entry! [^ZipOutputStream out path data]
  (.putNextEntry out (ZipEntry. path))
  (let [bytes (.getBytes (json/write-str data) "UTF-8")]
    (.write out bytes))
  (.closeEntry out))

;; ---------------------------------------------------------------------------
;; Shape factories — produce maps that satisfy the v3 JSON shape schema.
;; All UUIDs are strings (as the v3 encoder produces).
;; ---------------------------------------------------------------------------

(defn- new-uuid [] (str (UUID/randomUUID)))

(defn- base-shape
  [page-id frame-id parent-id {:keys [type name x y w h]}]
  {:id             (new-uuid)
   :page-id        page-id
   :type           (clojure.core/name type)
   :name           (or name (str (clojure.core/name type) "-" (rand-int 999999)))
   :parent-id      parent-id
   :frame-id       frame-id
   :selrect        {:x x :y y :x1 x :y1 y
                    :x2 (+ x w) :y2 (+ y h)
                    :width w :height h}
   :points         [{:x x :y y} {:x (+ x w) :y y}
                    {:x (+ x w) :y (+ y h)} {:x x :y (+ y h)}]
   :transform      {:a 1 :b 0 :c 0 :d 1 :e 0 :f 0}
   :transform-inverse {:a 1 :b 0 :c 0 :d 1 :e 0 :f 0}
   :rotation       0
   :opacity        1
   :blend-mode     "normal"
   :hidden         false
   :blocked        false
   :locked         false
   :proportion     1.0
   :proportion-lock false
   :constraints-h  "left"
   :constraints-v  "top"})

(defn- rect-shape [page-id frame-id parent-id x y w h color-hex]
  (-> (base-shape page-id frame-id parent-id
                  {:type :rect :x x :y y :w w :h h})
      (assoc :fills
             [{:fill-color color-hex
               :fill-opacity 1
               :fill-color-ref-file nil
               :fill-color-ref-id nil}])
      (assoc :strokes [])))

(defn- ellipse-shape [page-id frame-id parent-id x y w h color-hex]
  (-> (base-shape page-id frame-id parent-id
                  {:type :ellipse :x x :y y :w w :h h})
      (assoc :fills
             [{:fill-color color-hex
               :fill-opacity 1
               :fill-color-ref-file nil
               :fill-color-ref-id nil}])
      (assoc :strokes [])))

(defn- path-shape [page-id frame-id parent-id x y w h]
  ;; A simple closed rectangular path — the rasteriser exercises the path
  ;; code-path even for axis-aligned shapes.
  (let [content [{:command "M" :x x :y y}
                 {:command "L" :x (+ x w) :y y}
                 {:command "L" :x (+ x w) :y (+ y h)}
                 {:command "L" :x x :y (+ y h)}
                 {:command "Z"}]]
    (-> (base-shape page-id frame-id parent-id
                    {:type :path :x x :y y :w w :h h})
        (assoc :content content)
        (assoc :fills [])
        (assoc :strokes
               [{:stroke-color "#1a73e8"
                 :stroke-opacity 1
                 :stroke-width 2
                 :stroke-alignment "inner"
                 :stroke-type "dashed"
                 :stroke-cap-start "none"
                 :stroke-cap-end "none"}]))))

(defn- text-shape [page-id frame-id parent-id x y w h text]
  (-> (base-shape page-id frame-id parent-id
                  {:type :text :x x :y y :w w :h h})
      (assoc :content
             {:type "root"
              :children
              [{:type "paragraph-set"
                :children
                [{:type "paragraph"
                  :children
                  [{:text text
                    :fills [{:fill-color "#000000" :fill-opacity 1}]
                    :font-family "Source Sans Pro"
                    :font-size "14"
                    :font-weight "400"
                    :font-style "normal"
                    :text-decoration "none"
                    :letter-spacing "0"
                    :line-height "1.2"
                    :text-transform "none"}]}]}]})
      (assoc :fills [])
      (assoc :strokes [])))

(defn- frame-shape
  [page-id parent-id {:keys [id x y w h name children]}]
  (-> (base-shape page-id id parent-id
                  {:type :frame :name name :x x :y y :w w :h h})
      (assoc :id id)
      (assoc :frame-id (or parent-id id))
      (assoc :fills [{:fill-color "#ffffff" :fill-opacity 1}])
      (assoc :strokes [])
      (assoc :shapes (mapv :id children))
      (assoc :clip-content true)
      (assoc :show-content false)))

(defn- group-shape [page-id frame-id parent-id children x y w h]
  (-> (base-shape page-id frame-id parent-id
                  {:type :group :x x :y y :w w :h h})
      (assoc :shapes (mapv :id children))
      (assoc :fills [])
      (assoc :strokes [])))

;; ---------------------------------------------------------------------------
;; Page builders — each returns {:page-meta <map> :objects <map id→shape>}
;; ---------------------------------------------------------------------------

(def ^:private root-id "00000000-0000-0000-0000-000000000000")

(defn- root-shape [page-id]
  {:id         root-id
   :page-id    page-id
   :type       "frame"
   :name       "Root Frame"
   :parent-id  nil
   :frame-id   root-id
   :shapes     []
   :selrect    {:x 0 :y 0 :x1 0 :y1 0 :x2 0 :y2 0 :width 0 :height 0}
   :points     []
   :transform  {:a 1 :b 0 :c 0 :d 1 :e 0 :f 0}
   :transform-inverse {:a 1 :b 0 :c 0 :d 1 :e 0 :f 0}
   :rotation   0
   :opacity    1
   :blend-mode "normal"
   :hidden     false
   :blocked    false
   :locked     false
   :fills      []
   :strokes    []})

(defn- build-dense-frames-page
  "Page 1: 3 frames × 700 shapes = 2100 + 3 frames."
  [page-id]
  (let [frame-count 3
        per-frame   700
        colors      ["#e8f0fe" "#fce8e6" "#e6f4ea" "#fff3e0" "#f3e5f5"
                     "#e0f2f1" "#fce4ec" "#ede7f6" "#e3f2fd" "#f9fbe7"]]
    (loop [fi 0 all-shapes [] all-frame-ids []]
      (if (= fi frame-count)
        (let [root (-> (root-shape page-id)
                       (assoc :shapes all-frame-ids))]
          {:shapes all-shapes :root root})
        (let [frame-id   (new-uuid)
              frame-x    (* fi 2200)
              frame-w    2000
              frame-h    8000
              ;; Build leaf shapes
              leaf-shapes
              (for [i (range per-frame)]
                (let [col (mod i 10)
                      row (quot i 10)
                      x (+ frame-x 40 (* col 190))
                      y (+ 40 (* row 110))
                      w 170 h 90]
                  (if (even? i)
                    (rect-shape page-id frame-id frame-id
                                x y w h (nth colors (mod i (count colors))))
                    (ellipse-shape page-id frame-id frame-id
                                   x y w h (nth colors (mod (inc i) (count colors)))))))
              frame
              (frame-shape page-id root-id
                            {:id frame-id
                             :x frame-x :y 0
                             :w frame-w :h frame-h
                             :name (str "Dense Frame " (inc fi))
                             :children leaf-shapes})]
          (recur (inc fi)
                 (concat all-shapes (cons frame leaf-shapes))
                 (conj all-frame-ids frame-id)))))))

(defn- build-stroke-paths-page
  "Page 2: 2000 paths with dashed strokes."
  [page-id]
  (let [n 2000
        cols 40
        shapes
        (for [i (range n)]
          (let [col (mod i cols)
                row (quot i cols)
                x (* col 120) y (* row 80)
                w 100 h 60]
            (path-shape page-id root-id root-id x y w h)))
        root (-> (root-shape page-id)
                 (assoc :shapes (mapv :id shapes)))]
    {:shapes shapes :root root}))

(defn- build-typography-page
  "Page 3: 2000 text shapes with diverse content."
  [page-id]
  (let [n 2000
        cols 20
        samples ["The quick brown fox" "Logos design system"
                 "Performance matters" "10,000 shapes" "Phase 1 complete"
                 "WebGL rendering" "Adaptive tiling" "CRDT sync"]
        shapes
        (for [i (range n)]
          (let [col (mod i cols)
                row (quot i cols)
                x (* col 200) y (* row 60)
                w 180 h 50
                txt (nth samples (mod i (count samples)))]
            (text-shape page-id root-id root-id x y w h txt)))
        root (-> (root-shape page-id)
                 (assoc :shapes (mapv :id shapes)))]
    {:shapes shapes :root root}))

(defn- build-nested-groups-page
  "Page 4: deeply nested groups — 8 levels deep, 2048 leaf shapes."
  [page-id]
  (letfn [(build-tree [depth x y w h]
            (if (zero? depth)
              ;; leaf
              [(rect-shape page-id root-id root-id x y w h "#4285f4")]
              ;; group split into 2 halves
              (let [half-w   (/ w 2)
                    left     (build-tree (dec depth) x y half-w h)
                    right    (build-tree (dec depth) (+ x half-w) y half-w h)
                    children (concat left right)
                    gx x gy y gw w gh h
                    grp      (group-shape page-id root-id root-id
                                          children gx gy gw gh)]
                (cons grp children))))]
    ;; 2^8 = 256 leaves per tree, 8 trees = 2048 leaves
    (loop [ti 0 all-shapes []]
      (if (= ti 8)
        (let [root (-> (root-shape page-id)
                       (assoc :shapes (mapv :id (filter #(= root-id (:parent-id %))
                                                         all-shapes))))]
          {:shapes all-shapes :root root})
        (let [tree-shapes (build-tree 8 (* ti 1200) 0 1100 1000)]
          (recur (inc ti) (concat all-shapes tree-shapes)))))))

(defn- build-grid-layout-page
  "Page 5: 2000 leaf rects inside a single grid-layout frame."
  [page-id]
  (let [frame-id (new-uuid)
        n        2000
        cols     40
        leaf-shapes
        (for [i (range n)]
          (let [col (mod i cols)
                row (quot i cols)
                x (* col 62)
                y (* row 62)]
            (rect-shape page-id frame-id frame-id x y 56 56 "#34a853")))
        frame
        (frame-shape page-id root-id
                     {:id frame-id
                      :x 0 :y 0 :w 2500 :h 3200
                      :name "Grid Layout Frame"
                      :children leaf-shapes})
        root (-> (root-shape page-id)
                 (assoc :shapes [frame-id]))]
    {:shapes (cons frame leaf-shapes) :root root}))

;; ---------------------------------------------------------------------------
;; Assemble and write the v3 ZIP archive.
;; ---------------------------------------------------------------------------


(defn generate!
  [output-path]
  (let [file-id   (new-uuid)
        page-defs [;; [name builder-fn]
                   ["Dense Frames"   build-dense-frames-page]
                   ["Stroke Paths"   build-stroke-paths-page]
                   ["Typography"     build-typography-page]
                   ["Nested Groups"  build-nested-groups-page]
                   ["Grid Layout"    build-grid-layout-page]]
        ;; Build each page
        built-pages
        (mapv (fn [[name builder-fn]]
                (let [page-id (new-uuid)
                      result  (builder-fn page-id)]
                  {:page-id   page-id
                   :page-name name
                   :shapes    (:shapes result)
                   :root      (:root result)}))
              page-defs)

        manifest
        {:type         "penpot/export-files"
         :version      1
         :generated-by "logos/p1.7-benchmark-generator"
         :refer        "penpot"
         :files        [{:id       file-id
                         :name     "Logos Memory Benchmark"
                         :features ["fdata/shape-data-type" "fdata/path-data"]}]}

        total-shapes (reduce + (map #(count (:shapes %)) built-pages))]

    (println (str "Generating benchmark file: " output-path))
    (println (str "  Pages: " (count built-pages)))
    (println (str "  Total shapes: " total-shapes))

    (io/make-parents output-path)

    (with-open [fos (FileOutputStream. (File. ^String output-path))
                zos (ZipOutputStream. fos)]

      ;; manifest.json
      (write-entry! zos "manifest.json" manifest)

      ;; files/<file-id>.json — omit :data (shapes live in per-page entries)
      (write-entry! zos (str "files/" file-id ".json")
                    {:id          file-id
                     :name        "Logos Memory Benchmark"
                     :revn        0
                     :is-shared   false
                     :modified-at (.toString (java.time.Instant/now))
                     :created-at  (.toString (java.time.Instant/now))
                     :features    ["fdata/shape-data-type" "fdata/path-data"]
                     :pages       (mapv :page-id built-pages)})

      ;; One entry per page, one entry per shape.
      (doseq [[idx {:keys [page-id page-name shapes root]}]
              (map-indexed vector built-pages)]

        ;; Page header (no :objects — those live as individual ZipEntries).
        (write-entry! zos
                      (str "files/" file-id "/pages/" page-id ".json")
                      {:id      page-id
                       :name    page-name
                       :index   idx
                       :options {}})

        ;; Root frame
        (write-entry! zos
                      (str "files/" file-id "/pages/" page-id
                           "/" root-id ".json")
                      root)

        ;; Individual shapes (include :page-id — v3 writer adds it per shape)
        (doseq [shape shapes]
          (write-entry! zos
                        (str "files/" file-id "/pages/" page-id
                             "/" (:id shape) ".json")
                        shape))))

    (println (str "Done. Output: " output-path))
    (println (str "File size: "
                  (long (/ (.length (File. ^String output-path)) 1024))
                  " KB"))))

(defn -main [& args]
  (let [output (or (second (drop-while #(not= "--output" %) args))
                   "fixtures/large-canvas.penpot")]
    (generate! output)))
