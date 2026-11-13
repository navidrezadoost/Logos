const test = require('node:test');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const Ajv2020 = require('ajv/dist/2020');
const contract = require('../../contracts/design-token/v0.1/schema.json');

const ajv = new Ajv2020({ allErrors: true, strict: false });
ajv.addSchema(contract);
const validate = (name) => ajv.getSchema(`${contract.$id}#/$defs/${name}`);
const validIntent = validate('userIntent');
const validContext = validate('documentContext');
const validSnapshot = validate('tokenSnapshot');
const validPlan = validate('proposedPlan');
const validValidation = validate('validationResult');
const validCommit = validate('commitRequest');
const validReceipt = validate('transactionReceipt');
const NS = 'document:test';
const DOCUMENT = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const SHAPE = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const TOKEN = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc';
const INTENT = 'dddddddd-dddd-4ddd-8ddd-dddddddddddd';
const PLAN = 'eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee';
const TRANSACTION = 'ffffffff-ffff-4fff-8fff-ffffffffffff';
const direct = { kind: 'literal', value: { type: 'color', value: { space: 'srgb', hex: '#2563eb', alpha: 1 } } };

function makeInput() {
  const intent = {
    protocolVersion: '0.1.0', intentId: INTENT,
    text: 'Save the selected fill as a reusable color and bind it.',
    requestedCapabilities: ['createToken', 'bindProperty'],
  };
  const documentContext = {
    protocolVersion: '0.1.0', documentId: DOCUMENT, revision: 'r1',
    selectedShapeIds: [SHAPE],
    propertyValues: [{ shapeId: SHAPE, propertyPath: 'fill', value: direct }],
    allowedNamespaces: [NS], permissions: ['readDocument', 'readTokens', 'proposeChanges'],
  };
  const snapshot = {
    contractVersion: '0.1.0', status: 'PROPOSED', revision: 'r1', documentNamespace: NS,
    tokens: [], collections: [], sets: [], contextDimensions: [], activeSetIds: [],
  };
  const binding = {
    kind: 'binding', target: { namespace: NS, tokenId: TOKEN },
    modeStrategy: { kind: 'inherit' }, fallback: direct.value,
  };
  const plan = {
    protocolVersion: '0.1.0', planId: PLAN, baseRevision: 'r1', intentId: INTENT,
    summary: 'Create and bind a color token.',
    operations: [
      { kind: 'createToken', definition: {
        id: TOKEN, namespace: NS, name: 'color/primary', type: 'color',
        values: [{ when: {}, expression: { kind: 'literal', value: direct.value } }],
      } },
      { kind: 'setProperty', shapeId: SHAPE, propertyPath: 'fill', expectedProperty: direct, nextProperty: binding },
    ],
    assumptions: [], requiresUserApproval: true,
  };
  return { intent, documentContext, snapshot, plan };
}

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
}
function digest(value) {
  return createHash('sha256').update(JSON.stringify(canonical(value))).digest('hex');
}
function diagnostic(code) { return { code, severity: 'error', message: code }; }

function validatePlan(plan, context, snapshot) {
  const diagnostics = [];
  if (!validPlan(plan)) diagnostics.push(diagnostic('UNSUPPORTED_FEATURE'));
  if (plan.baseRevision !== context.revision || snapshot.revision !== context.revision) diagnostics.push(diagnostic('STALE_REVISION'));
  if (!context.permissions.includes('proposeChanges')) diagnostics.push(diagnostic('SCOPE_DENIED'));
  const created = new Set(snapshot.tokens.map((token) => `${token.namespace}:${token.id}`));
  const properties = new Map(context.propertyValues.map((item) => [`${item.shapeId}:${item.propertyPath}`, item.value]));
  for (const operation of plan.operations) {
    if (operation.kind === 'createToken') {
      const key = `${operation.definition.namespace}:${operation.definition.id}`;
      if (!context.allowedNamespaces.includes(operation.definition.namespace) || created.has(key)) diagnostics.push(diagnostic('SCOPE_DENIED'));
      created.add(key);
    }
    if (operation.kind === 'setProperty') {
      const key = `${operation.shapeId}:${operation.propertyPath}`;
      const current = properties.get(key);
      if (!context.selectedShapeIds.includes(operation.shapeId) || !context.allowedNamespaces.includes(NS)) diagnostics.push(diagnostic('SCOPE_DENIED'));
      if (JSON.stringify(current ?? null) !== JSON.stringify(operation.expectedProperty ?? null)) diagnostics.push(diagnostic('STALE_REVISION'));
      properties.set(key, operation.nextProperty);
    }
  }
  const unique = [...new Map(diagnostics.map((d) => [d.code, d])).values()];
  const result = {
    protocolVersion: '0.1.0', planId: plan.planId, baseRevision: plan.baseRevision,
    status: unique.length ? 'REJECTED' : 'READY', planDigest: digest(plan),
    impact: {
      createdTokens: plan.operations.filter((op) => op.kind === 'createToken').length,
      updatedTokens: plan.operations.filter((op) => op.kind === 'setTokenValue').length,
      boundProperties: plan.operations.filter((op) => op.kind === 'setProperty' && op.nextProperty.kind === 'binding').length,
      affectedPropertyPaths: [...new Set(plan.operations.filter((op) => op.kind === 'setProperty').map((op) => op.propertyPath))],
      contextsEvaluated: [], diagnostics: unique,
    },
    diagnostics: unique,
  };
  assert.equal(validValidation(result), true, JSON.stringify(validValidation.errors));
  return result;
}

function previewPlan(plan, context, snapshot, validation) {
  if (validation.status !== 'READY' || validation.planDigest !== digest(plan)) return null;
  const nextContext = structuredClone(context);
  const nextSnapshot = structuredClone(snapshot);
  for (const operation of plan.operations) {
    if (operation.kind === 'createToken') nextSnapshot.tokens.push(structuredClone(operation.definition));
    if (operation.kind === 'setProperty') {
      const item = nextContext.propertyValues.find((p) => p.shapeId === operation.shapeId && p.propertyPath === operation.propertyPath);
      if (item) item.value = structuredClone(operation.nextProperty);
    }
  }
  return { documentContext: nextContext, snapshot: nextSnapshot };
}

test('agent protocol messages are typed and create-token plus binding is one revision-bound plan', () => {
  const input = makeInput();
  assert.equal(validIntent(input.intent), true, JSON.stringify(validIntent.errors));
  assert.equal(validContext(input.documentContext), true, JSON.stringify(validContext.errors));
  assert.equal(validSnapshot(input.snapshot), true, JSON.stringify(validSnapshot.errors));
  assert.equal(validPlan(input.plan), true, JSON.stringify(validPlan.errors));
  assert.equal(input.plan.operations.length, 2);
  assert.equal(input.plan.requiresUserApproval, true);
});

test('deterministic validation produces a typed impact and supports immutable preview', () => {
  const input = makeInput();
  const before = structuredClone(input);
  const first = validatePlan(input.plan, input.documentContext, input.snapshot);
  const second = validatePlan(input.plan, input.documentContext, input.snapshot);
  assert.equal(first.status, 'READY');
  assert.deepEqual(first, second);
  assert.equal(first.impact.createdTokens, 1);
  assert.equal(first.impact.boundProperties, 1);
  const preview = previewPlan(input.plan, input.documentContext, input.snapshot, first);
  assert.equal(preview.snapshot.tokens.length, 1);
  assert.equal(preview.documentContext.propertyValues[0].value.kind, 'binding');
  assert.deepEqual(input, before, 'planning, validation, and preview must not mutate the source');
});

test('stale revisions, missing proposal permission, and unexpected pre-state reject without mutation', () => {
  const input = makeInput();
  const original = structuredClone(input);
  const stale = structuredClone(input);
  stale.plan.baseRevision = 'r0';
  assert.equal(validatePlan(stale.plan, stale.documentContext, stale.snapshot).status, 'REJECTED');
  const unauthorized = structuredClone(input);
  unauthorized.documentContext.permissions = ['readDocument'];
  assert.equal(validatePlan(unauthorized.plan, unauthorized.documentContext, unauthorized.snapshot).status, 'REJECTED');
  const conflict = structuredClone(input);
  conflict.documentContext.propertyValues[0].value = { kind: 'literal', value: { type: 'color', value: { space: 'srgb', hex: '#000000', alpha: 1 } } };
  assert.equal(validatePlan(conflict.plan, conflict.documentContext, conflict.snapshot).status, 'REJECTED');
  assert.equal(previewPlan(stale.plan, stale.documentContext, stale.snapshot, validatePlan(stale.plan, stale.documentContext, stale.snapshot)), null);
  assert.deepEqual(input, original);
});

test('agent plans cannot carry executable code or disable user approval', () => {
  const input = makeInput();
  const withCode = { ...input.plan, script: 'mutate registry' };
  const withoutApproval = { ...input.plan, requiresUserApproval: false };
  assert.equal(validPlan(withCode), false);
  assert.equal(validPlan(withoutApproval), false);
});

test('commit gate requires explicit approval, matching revision, and a ready validation digest', () => {
  const input = makeInput();
  const validation = validatePlan(input.plan, input.documentContext, input.snapshot);
  const commit = {
    protocolVersion: '0.1.0', planId: input.plan.planId, expectedRevision: 'r1',
    validationDigest: validation.planDigest, explicitUserApproval: true, idempotencyKey: 'approved-commit-key-001',
  };
  assert.equal(validCommit(commit), true, JSON.stringify(validCommit.errors));
  assert.equal(validCommit({ ...commit, explicitUserApproval: false }), false);
  assert.equal(commit.expectedRevision === validation.baseRevision && validation.status === 'READY', true);
});

test('transaction receipt carries auditable revision, operation, undo, and replay references', () => {
  const receipt = {
    protocolVersion: '0.1.0', transactionId: 'ffffffff-ffff-4fff-8fff-ffffffffffff', status: 'APPLIED',
    baseRevision: 'r1', resultingRevision: 'r2', operationIds: ['op-1', 'op-2'], actorId: 'user-1',
    approvedByUser: true, undoReference: 'undo:r2', replayReference: 'replay:r2', diagnostics: [],
  };
  assert.equal(validReceipt(receipt), true, JSON.stringify(validReceipt.errors));
  assert.equal(validReceipt({ ...receipt, approvedByUser: false }), false);
});

test('unknown extension payloads are retained while unrecognized contract fields fail closed', () => {
  const input = makeInput();
  const withExtension = { ...input.snapshot, extensions: { 'vendor.example': { value: ['opaque', 7] } } };
  assert.equal(validSnapshot(withExtension), true);
  assert.deepEqual(structuredClone(withExtension).extensions, withExtension.extensions);
  assert.equal(validSnapshot({ ...withExtension, unrecognized: true }), false);
});
