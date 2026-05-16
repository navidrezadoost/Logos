#!/usr/bin/env node
// Generates src/app/main/ui/ds/foundations/assets/duotone_icon.cljs
// from resources/images/icons/duotone/*.svg

import { readdir, writeFile } from "node:fs/promises";
import path from "node:path";

const DUOTONE_DIR = "resources/images/icons/duotone/";
const OUTPUT_PATH =
  "src/app/main/ui/ds/foundations/assets/duotone_icon.cljs";

// Sanitize an SVG basename (no ext) into a valid ClojureScript identifier.
// Matches the sanitizeDuotoneName() logic in _helpers.js.
function toClojureIdent(name) {
  // Replace underscores with hyphens for ClojureScript conventions
  let ident = name.replace(/_/g, "-");
  // If starts with a digit, prefix to make a valid identifier
  if (/^[0-9]/.test(ident)) {
    ident = "i-" + ident;
  }
  return ident;
}

async function main() {
  const entries = await readdir(DUOTONE_DIR);
  const svgs = entries
    .filter((f) => f.endsWith(".svg"))
    .map((f) => path.basename(f, ".svg"))
    .sort();

  const defs = svgs
    .map((name) => {
      const ident = toClojureIdent(name);
      // Value equals the identifier (matching sprite ID sans "icon-dt-" prefix).
      return `(def ^:icon-id ${ident} "${ident}")`;
    })
    .join("\n");

  // Build the icon-list set for validation and the component
  const cljs = `;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC
;;
;; AUTO-GENERATED — do not edit by hand.
;; Regenerate with: node scripts/generate-duotone-icons.mjs

(ns app.main.ui.ds.foundations.assets.duotone-icon
  (:require-macros
   [app.common.data.macros :as dm]
   [app.main.style :as stl]
   [app.main.ui.ds.foundations.assets.icon :refer [collect-icons]])
  (:require
   [rumext.v2 :as mf]))

;; --- Icon IDs -----------------------------------------------------------
;; Each var holds the bare icon name used to construct the sprite href:
;;   #icon-dt-{name}

${defs}

;; --- Registry -----------------------------------------------------------

(def icon-list
  "A collection of all duotone icons"
  (collect-icons))

;; --- Sizes --------------------------------------------------------------

(def ^:private ^:const icon-size-l 32)
(def ^:private ^:const icon-size-m 16)
(def ^:private ^:const icon-size-s 12)

;; --- Schema -------------------------------------------------------------

(def ^:private schema:duotone-icon
  [:map
   [:class {:optional true} [:maybe :string]]
   [:icon-id [:and :string [:fn #(contains? icon-list %)]]]
   [:size {:optional true}
    [:maybe [:enum "s" "m" "l"]]]])

;; --- Component ----------------------------------------------------------

(mf/defc duotone-icon*
  {::mf/schema schema:duotone-icon}
  [{:keys [icon-id size class] :rest props}]
  (let [props   (mf/spread-props props
                                 {:class [class (stl/css :icon)]
                                  :width icon-size-m
                                  :height icon-size-m})

        size-px (cond (= size "l") icon-size-l
                      (= size "s") icon-size-s
                      :else        icon-size-m)

        offset  (if (or (= size "s") (= size "m"))
                  (/ (- icon-size-m size-px) 2)
                  0)]

    [:> :svg props
     [:use {:href   (dm/str "#icon-dt-" icon-id)
            :width  size-px
            :height size-px
            :x      offset
            :y      offset}]]))
`;

  await writeFile(OUTPUT_PATH, cljs, "utf-8");
  console.log(
    `Written ${svgs.length} duotone icon defs to ${OUTPUT_PATH}`,
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
