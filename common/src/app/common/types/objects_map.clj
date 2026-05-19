;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.types.objects-map
  "Implements a specialized map-like data structure for store an UUID =>
  OBJECT mappings. The main purpose of this data structure is be able
  to serialize it on fressian as byte-array and have the ability to
  decode each field separatelly without the need to decode the whole
  map from the byte-array.

  It works transparently, so no aditional dynamic vars are needed.  It
  only works by reference equality and the hash-code is calculated
  properly from each value."

  (:require
   [app.common.fressian :as fres]
   [clojure.data.json :as json]
   [app.common.transit :as t]
   [clojure.core :as c]
   [clojure.core.protocols :as cp])
  (:import
      clojure.lang.Murmur3
      clojure.lang.RT
      java.util.Iterator))

(set! *warn-on-reflection* true)

(declare create)
(declare ^:private do-compact)

(defprotocol IObjectsMap
  (^:no-doc compact [this])
  (^:no-doc get-data [this] "retrieve internal data")
  (^:no-doc -hash-for-key [this key] "retrieve a hash for a key"))

(deftype ObjectsMapEntry [key omap]
     clojure.lang.IMapEntry
     (key [_] key)
     (getKey [_] key)

     (val [_]
       (get omap key))
     (getValue [_]
       (get omap key))

     clojure.lang.Indexed
     (nth [node n]
       (cond
         (== n 0) key
         (== n 1) (val node)
         :else    (throw (IllegalArgumentException. "Index out of bounds"))))

     (nth [node n not-found]
       (cond
         (== n 0) key
         (== n 1) (val node)
         :else    not-found))

     clojure.lang.IPersistentCollection
     (empty [_] [])
     (count [_] 2)
     (seq [this]
       (cons key (lazy-seq (cons (val this) nil))))
     (cons [this item]
       (.cons ^clojure.lang.IPersistentCollection (vec this) item))

     clojure.lang.IHashEq
     (hasheq [_]
       (-hash-for-key omap key)))

(deftype ObjectsMapIterator [^Iterator iterator omap]
     Iterator
     (hasNext [_]
       (.hasNext iterator))

     (next [_]
       (let [entry (.next iterator)]
         (ObjectsMapEntry. (key entry) omap))))

(deftype ObjectsMap [metadata cache
                        ^:unsynchronized-mutable data
                        ^:unsynchronized-mutable modified
                        ^:unsynchronized-mutable hash]

     Object
     (hashCode [this]
       (.hasheq ^clojure.lang.IHashEq this))

     cp/Datafiable
     (datafy [_]
       {:data data
        :cache cache
        :modified modified
        :hash hash})

     IObjectsMap
     (compact [this]
       (locking this
         (when modified
           (do-compact data cache
                       (fn [data']
                         (set! (.-modified this) false)
                         (set! (.-data this) data')))))
       this)

     (get-data [this]
       (compact this)
       data)

     (-hash-for-key [this key]
       (if (contains? cache key)
         (c/hash (get cache key))
         (c/hash (get this key))))

     json/JSONWriter
     (-write [this writter options]
       (json/-write (into {} this) writter options))

     clojure.lang.IHashEq
     (hasheq [this]
       (when-not hash
         (set! hash (Murmur3/hashUnordered this)))
       hash)

     clojure.lang.Seqable
     (seq [this]
       (RT/chunkIteratorSeq (.iterator ^Iterable this)))

     java.lang.Iterable
     (iterator [this]
       (ObjectsMapIterator. (.iterator ^Iterable data) this))

     clojure.lang.IPersistentCollection
     (equiv [this other]
       (and (instance? ObjectsMap other)
            (= (count this) (count other))
            (reduce-kv (fn [_ id _]
                         (let [this-val  (get this id)
                               other-val (get other id)
                               result    (= this-val other-val)]
                           (or result
                               (reduced false))))
                       true
                       data)))

     clojure.lang.IPersistentMap
     (cons [this o]
       (if (map-entry? o)
         (assoc this (key o) (val o))
         (if (vector? o)
           (assoc this (nth o 0) (nth o 1))
           (throw (UnsupportedOperationException. "invalid arguments to cons")))))

     (empty [_]
       (create))

     (containsKey [_ key]
       (.containsKey ^clojure.lang.IPersistentMap data key))

     (entryAt [this key]
       (ObjectsMapEntry. this key))

     (valAt [this key]
       (or (get cache key)
           (locking this
             (if (contains? data key)
               (let [value (get data key)
                     value (t/decode-str value)]
                 (set! (.-cache this) (assoc cache key value))
                 value)
               (do
                 (set! (.-cache this) (assoc cache key nil))
                 nil)))))

     (valAt [this key not-found]
       (if (.containsKey ^clojure.lang.IPersistentMap data key)
         (.valAt this key)
         not-found))

     (assoc [_ key val]
       (ObjectsMap. metadata
                    (assoc cache key val)
                    (assoc data key nil)
                    true
                    nil))

     (assocEx [_ _ _]
       (throw (UnsupportedOperationException. "method not implemented")))

     (without [_ key]
       (ObjectsMap. metadata
                    (dissoc cache key)
                    (dissoc data key)
                    true
                    nil))

     clojure.lang.Counted
     (count [_]
       (count data)))

(defn- do-compact
  [data cache update-fn]
  (let [new-data
        (persistent!
         (reduce-kv (fn [data id obj]
                      (if (nil? obj)
                        (assoc! data id (t/encode-str (get cache id)))
                        data))
                    (transient data)
                    data))]
    (update-fn new-data)
    nil))

(defn from-data
  [data]
  (ObjectsMap. {} {}
               data
               false
               nil))

(defn objects-map?
  [o]
  (instance? ObjectsMap o))

(defn create
  ([] (from-data {}))
  ([other]
   (cond
     (objects-map? other)
     (-> other get-data from-data)

     :else
     (throw (UnsupportedOperationException. "invalid arguments")))))

(defn wrap
  [objects]
  (if (instance? ObjectsMap objects)
    objects
    (->> objects
         (into (create))
         (compact))))

(fres/add-handlers!
    {:name "penpot/objects-map/v2"
     :class ObjectsMap
     :wfn (fn [n w o]
            (fres/write-tag! w n)
            (fres/write-object! w (get-data o)))
     :rfn (fn [r]
            (-> r fres/read-object! from-data))})

(t/add-handlers!
 {:id "penpot/objects-map/v2"
  :class ObjectsMap
  :wfn get-data
  :rfn from-data})
