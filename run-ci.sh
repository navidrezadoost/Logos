#!/bin/bash

set -e

echo "################ test common ################"
pushd common
pnpm install
pnpm run fmt:clj:check
pnpm run lint:clj
clojure -M:dev:test
pnpm run test
popd

echo "################ build + test logos-app (React/TypeScript) ################"
pushd logos-app
pnpm install
npx tsc --noEmit
npx vitest run
popd

echo "################ test backend ################"
pushd backend
pnpm install
pnpm run fmt:clj:check
pnpm run lint:clj
clojure -M:dev:test --reporter kaocha.report/documentation
popd

echo "################ test exporter ################"
pushd exporter
pnpm install
pnpm run fmt:clj:check
pnpm run lint:clj
popd

echo "################ test render-wasm ################"
pushd render-wasm
cargo fmt --check
./lint --debug
./test
popd
