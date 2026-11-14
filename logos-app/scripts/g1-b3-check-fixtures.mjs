import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { toRustCandidate } from "../src/document-core/react-adapter.ts";

const fixturePath = "src/document-core/fixtures";
const reactSnapshot = JSON.parse(readFileSync(`${fixturePath}/react-supported-v1.json`, "utf8"));
const expectedCandidate = JSON.parse(readFileSync(`${fixturePath}/rust-candidate-v1.json`, "utf8"));
const converted = toRustCandidate(reactSnapshot);
assert.equal(converted.ok, true, JSON.stringify(converted));
assert.deepEqual(converted.candidate, expectedCandidate);
console.log("G1-B3 shared React/Rust fixture mapping: PASS");
