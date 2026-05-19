;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.svg.path
  (:import app.common.svg.path.Parser
              app.common.svg.path.Parser$Segment)
  )

(defn arc->beziers
  "A function for convert Arcs to Beziers, used only for testing
  purposes."
  [x1 y1 x2 y2 fa fs rx ry phi]
  (Parser/arcToBeziers (double x1)
                               (double y1)
                               (double x2)
                               (double y2)
                               (double fa)
                               (double fs)
                               (double rx)
                               (double ry)
                               (double phi)))

(defn parse
  [path-str]
  (if (empty? path-str)
    path-str
    (into []
             (map (fn [segment]
                    (.toPersistentMap ^Parser$Segment segment)))
             (Parser/parse path-str))))
