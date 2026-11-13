const test = require('node:test');
const assert = require('node:assert/strict');
const Ajv2020 = require('ajv/dist/2020');
const schema = require('../../contracts/design-token/v0.1/schema.json');
const fixtureSchema = require('../../contracts/design-token/v0.1/fixtures/fixtures.schema.json');
const golden = require('../../contracts/design-token/v0.1/fixtures/golden.json');
const resolutionVectors = require('../../contracts/design-token/v0.1/fixtures/resolution-vectors.json');
const resolutionVectorsSchema = require('../../contracts/design-token/v0.1/fixtures/resolution-vectors.schema.json');

const ajv = new Ajv2020({ allErrors: true, strict: false });
ajv.addSchema(schema);
const validateSnapshot = ajv.getSchema(`${schema.$id}#/$defs/tokenSnapshot`);
const validateRequest = ajv.getSchema(`${schema.$id}#/$defs/resolveRequest`);
const validateResult = ajv.getSchema(`${schema.$id}#/$defs/resolveResult`);
const validateFixture = ajv.compile(fixtureSchema);
const validateResolutionVectors = ajv.compile(resolutionVectorsSchema);

const ids = {
  a: '11111111-1111-4111-8111-111111111111',
  b: '22222222-2222-4222-8222-222222222222',
  c: '33333333-3333-4333-8333-333333333333',
  collection: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
  light: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
  dark: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
  setA: 'dddddddd-dddd-4ddd-8ddd-dddddddddddd',
  setB: 'eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee',
  setC: 'ffffffff-ffff-4fff-8fff-ffffffffffff',
};
const ns = 'document:test';
const color = (hex) => ({ type: 'color', value: { space: 'srgb', hex, alpha: 1 } });
const number = (value) => ({ type: 'number', value });
const dimension = (value, unit) => ({ type: 'dimension', value, unit });
const emptySnapshot = () => ({
  contractVersion: '0.1.0', status: 'PROPOSED', revision: 'r1',
  documentNamespace: ns, tokens: [], collections: [], sets: [], contextDimensions: [], activeSetIds: [],
});
const context = (parts = {}) => ({
  explicit: parts.explicit || {}, ancestorsNearestFirst: parts.ancestors || [],
  page: parts.page || {}, document: parts.document || {},
});
const request = (property, propertyType, ctx = context()) => ({
  contractVersion: '0.1.0', snapshotRevision: 'r1', propertyType, property, context: ctx,
});
const token = (id, name, type, expression, extras = {}) => ({
  id, namespace: ns, name, type,
  values: [{ when: {}, expression }],
  ...extras,
});
const literalExpr = (value) => ({ kind: 'literal', value });
const refExpr = (id, namespace = ns) => ({ kind: 'reference', target: { namespace, tokenId: id } });

function error(code, trace = []) {
  return { code, trace };
}

function contextFor(snapshot, requestValue, binding) {
  const layers = [
    requestValue.context.document,
    requestValue.context.page,
    ...requestValue.context.ancestorsNearestFirst.slice().reverse(),
    requestValue.context.explicit,
  ];
  const effective = Object.assign({}, ...layers);
  const collectionId = snapshot.tokens.find((t) => t.id === binding.target.tokenId && t.namespace === binding.target.namespace)?.collectionId;
  const collection = snapshot.collections.find((c) => c.id === collectionId);
  if (collection) {
    const dim = collection.modeDimension;
    if (binding.modeStrategy.kind === 'explicit') {
      const mode = collection.modes.find((m) => m.id === binding.modeStrategy.modeId);
      if (!mode) return { effective, error: error('MISSING_MODE') };
      effective[dim] = mode.option;
    } else if (!Object.hasOwn(effective, dim)) {
      const mode = collection.modes.find((m) => m.id === collection.defaultModeId);
      if (!mode) return { effective, error: error('MISSING_MODE') };
      effective[dim] = mode.option;
    }
  }
  return { effective };
}

function resolve(snapshot, requestValue) {
  const diagnostics = [];
  const trace = [];
  let budgetNodes = 0;
  const definitionKeys = new Set();
  let invalidSnapshot = false;
  for (const definition of snapshot.tokens) {
    const key = `${definition.namespace}:${definition.id}:${definition.setId || ''}`;
    if (definitionKeys.has(key)) invalidSnapshot = true;
    definitionKeys.add(key);
  }
  if (invalidSnapshot) return resultShape('ERROR', undefined, undefined, [], {}, [{ code: 'INVALID_SNAPSHOT', severity: 'error', message: 'INVALID_SNAPSHOT' }]);
  const shapeIssue = snapshot.tokens.map((token) => token.values.map((item) => expressionShapeIssue(item.expression))).flat().find(Boolean);
  if (shapeIssue) return resultShape('ERROR', undefined, undefined, [], {}, [{ code: shapeIssue, severity: 'error', message: shapeIssue }]);
  function expressionShapeIssue(expr, depth = 1, count = { value: 0 }) {
    count.value += 1;
    if (count.value > 256 || depth > 32) return 'EXPRESSION_BUDGET';
    if (expr.kind !== 'operation') return null;
    const validCount = ['subtract', 'divide'].includes(expr.operator) ? expr.arguments.length === 2 :
      expr.operator === 'clamp' ? expr.arguments.length === 3 : expr.arguments.length >= 2;
    if (!validCount) return 'EXPRESSION_INVALID';
    for (const child of expr.arguments) {
      const issue = expressionShapeIssue(child, depth + 1, count);
      if (issue) return issue;
    }
    return null;
  }
  function fail(code) { return { error: error(code, trace.slice()) }; }
  function evalExpr(expr, expectedType, effective, stack, depth) {
    budgetNodes += 1;
    if (budgetNodes > 256 || depth > 32) return fail('EXPRESSION_BUDGET');
    if (expr.kind === 'literal') {
      if (expectedType && expr.value.type !== expectedType) return fail('TYPE_MISMATCH');
      return { value: expr.value };
    }
    if (expr.kind === 'reference') return resolveRef(expr.target, expectedType, effective, stack, depth);
    const args = expr.arguments.map((arg) => evalExpr(arg, null, effective, stack, depth + 1));
    const bad = args.find((arg) => arg.error);
    if (bad) return bad;
    const values = args.map((arg) => arg.value);
    if (values.some((v) => !['number', 'dimension'].includes(v.type))) return fail('TYPE_MISMATCH');
    if ((expr.operator === 'clamp' && values.length !== 3) ||
        (['subtract', 'divide'].includes(expr.operator) && values.length !== 2)) return fail('EXPRESSION_INVALID');
    const dimensional = values.filter((v) => v.type === 'dimension');
    const compatible = (left, right) => left === right || ['ms', 's'].includes(left) && ['ms', 's'].includes(right);
    const canonical = (v) => v.unit === 's' ? v.value * 1000 : v.value;
    if (['add', 'subtract', 'min', 'max', 'clamp'].includes(expr.operator)) {
      if (!values.every((v) => v.type === values[0].type) ||
          (values[0].type === 'dimension' && !values.every((v) => compatible(v.unit, values[0].unit)))) return fail('TYPE_MISMATCH');
    }
    const nums = values.map((v) => v.type === 'dimension' && ['ms', 's'].includes(v.unit) ? canonical(v) : v.value);
    let value;
    if (expr.operator === 'add') value = nums.reduce((n, v) => n + v, 0);
    else if (expr.operator === 'subtract') value = nums[0] - nums[1];
    else if (expr.operator === 'min') value = Math.min(...nums);
    else if (expr.operator === 'max') value = Math.max(...nums);
    else if (expr.operator === 'clamp' && values.length === 3) {
      if (nums[0] > nums[2]) return fail('EXPRESSION_INVALID');
      value = Math.max(nums[0], Math.min(nums[1], nums[2]));
    }
    else if (expr.operator === 'multiply') {
      if (dimensional.length > 1) return fail('TYPE_MISMATCH');
      value = values.reduce((n, v) => n * v.value, 1);
    } else if (expr.operator === 'divide') {
      if (values[1].value === 0) return fail('DIVISION_BY_ZERO');
      if (dimensional.length === 2 && !compatible(values[0].unit, values[1].unit)) return fail('TYPE_MISMATCH');
      value = dimensional.length === 2 ? canonical(values[0]) / canonical(values[1]) : nums[0] / nums[1];
    } else return fail('EXPRESSION_INVALID');
    if (!Number.isFinite(value) || Math.abs(value) > 1_000_000_000_000) return fail('NUMERIC_RANGE');
    const resultType = expr.operator === 'divide' && dimensional.length === 2 ? 'number' : dimensional.length ? 'dimension' : 'number';
    const resultUnit = ['ms', 's'].includes(values.find((v) => v.type === 'dimension')?.unit) ? 'ms' : values.find((v) => v.type === 'dimension')?.unit;
    const result = resultType === 'dimension' ? dimension(value, resultUnit) : number(value);
    if (expectedType && result.type !== expectedType) return fail('TYPE_MISMATCH');
    return { value: result };
  }
  function matches(when, effective) {
    return Object.entries(when).every(([key, value]) => effective[key] === value);
  }
  function resolveRef(ref, expectedType, effective, stack, depth) {
    const key = `${ref.namespace}:${ref.tokenId}`;
    if (stack.length >= 32) return fail('ALIAS_BUDGET');
    trace.push(ref);
    if (stack.includes(key)) return fail('ALIAS_CYCLE');
    const defs = snapshot.tokens.filter((item) => item.id === ref.tokenId && item.namespace === ref.namespace);
    if (!defs.length) return fail('MISSING_TOKEN');
    const baseDefs = defs.filter((item) => !item.setId);
    const overlayDefs = snapshot.activeSetIds.flatMap((setId, index) => {
      const set = snapshot.sets.find((item) => item.id === setId);
      if (!set || !matches(set.when, effective)) return [];
      return defs.filter((item) => item.setId === setId).map((item) => ({ item, index }));
    });
    const chosenOverlay = overlayDefs.sort((a, b) => a.index - b.index).at(-1)?.item;
    const definition = chosenOverlay || baseDefs[0];
    if (!definition) return fail('MISSING_TOKEN');
    if (expectedType && definition.type !== expectedType) return fail('TYPE_MISMATCH');
    const candidates = definition.values.filter((item) => matches(item.when, effective));
    if (!candidates.length) return fail('MISSING_MODE');
    const specificity = Math.max(...candidates.map((item) => Object.keys(item.when).length));
    const winners = candidates.filter((item) => Object.keys(item.when).length === specificity);
    if (winners.length !== 1) return fail('CONTEXT_CONFLICT');
    const resolved = evalExpr(winners[0].expression, definition.type, effective, [...stack, key], depth + 1);
    if (resolved.error) return resolved;
    if (expectedType && resolved.value.type !== expectedType) return fail('TYPE_MISMATCH');
    return resolved;
  }

  const prop = requestValue.property;
  let result;
  let effective = Object.assign({}, requestValue.context.document, requestValue.context.page,
    ...requestValue.context.ancestorsNearestFirst.slice().reverse(), requestValue.context.explicit);
  if (prop.kind === 'literal') {
    result = prop.value.type === requestValue.propertyType ? { value: prop.value } : fail('TYPE_MISMATCH');
    if (!result.error) return resultShape('RESOLVED', result.value, 'DIRECT', [], effective, diagnostics);
  } else if (prop.localOverride) {
    result = prop.localOverride.type === requestValue.propertyType ? { value: prop.localOverride } : fail('TYPE_MISMATCH');
    if (!result.error) return resultShape('RESOLVED', result.value, 'LOCAL_OVERRIDE', [], effective, diagnostics);
  } else {
    const contextResult = contextFor(snapshot, requestValue, prop);
    effective = contextResult.effective;
    if (contextResult.error) result = { error: contextResult.error };
    else if (Object.keys(effective).length > 8) {
      effective = {};
      result = fail('CONTEXT_BUDGET');
    } else result = resolveRef(prop.target, requestValue.propertyType, effective, [], 0);
    if (!result.error) return resultShape('RESOLVED', result.value, 'TOKEN', trace, effective, diagnostics);
  }
  const problem = result.error;
  if (prop.kind === 'binding' && prop.fallback && prop.fallback.type === requestValue.propertyType) {
    diagnostics.push({ code: problem.code, severity: 'warning', message: problem.code, tokenChain: trace.slice() });
    return resultShape('RESOLVED', prop.fallback, 'FALLBACK', trace, effective, diagnostics);
  }
  diagnostics.push({ code: problem.code, severity: 'error', message: problem.code, tokenChain: trace.slice() });
  return resultShape('ERROR', undefined, undefined, trace, effective, diagnostics);
}

function resultShape(status, value, source, trace, effectiveContext, diagnostics) {
  return {
    contractVersion: '0.1.0', snapshotRevision: 'r1', status,
    ...(value ? { value } : {}), ...(source ? { source } : {}),
    diagnostics, dependencyTrace: trace, effectiveContext,
  };
}

function snapshotForVector(vector) {
  const s = emptySnapshot();
  const scenario = vector.scenario;
  if (scenario.kind === 'alias-chain') {
    const chain = [ids.a, ids.b, ids.c];
    s.tokens = [
      token(chain[0], 'semantic/primary', 'color', refExpr(chain[1])),
      token(chain[1], 'primitive/primary', 'color', refExpr(chain[2])),
      token(chain[2], 'primitive/value', 'color', literalExpr(color(scenario.terminalHex))),
    ];
  } else if (scenario.kind === 'alias-budget') {
    const chain = Array.from({ length: scenario.hops + 1 }, (_, index) => `00000000-0000-4000-8000-${String(index + 1).padStart(12, '0')}`);
    s.tokens = chain.map((id, index) => token(id, `chain/${index}`, 'color', index === chain.length - 1
      ? literalExpr(color('#16a34a')) : refExpr(chain[index + 1])));
  } else if (scenario.kind === 'missing-fallback') {
    // Deliberately no token with the target ID.
  } else if (scenario.kind === 'missing-no-fallback' || scenario.kind === 'context-budget') {
    // Deliberately no token with the target ID.
  } else if (scenario.kind === 'alias-cycle') {
    s.tokens = [token(ids.a, 'a', 'color', refExpr(ids.b)), token(ids.b, 'b', 'color', refExpr(ids.a))];
  } else if (scenario.kind === 'modes') {
    s.collections = [{ id: ids.collection, name: 'Theme', modeDimension: 'mode', defaultModeId: ids.light,
      modes: [{ id: ids.light, option: 'light' }, { id: ids.dark, option: 'dark' }] }];
    s.tokens = [token(ids.a, 'surface', 'color', literalExpr(color('#ffffff')), { collectionId: ids.collection,
      values: [{ when: { mode: 'light' }, expression: literalExpr(color('#ffffff')) }, { when: { mode: 'dark' }, expression: literalExpr(color('#111111')) }] })];
  } else if (scenario.kind === 'dimensions') {
    s.tokens = [token(ids.a, 'brand/primary', 'color', literalExpr(color('#123456')),
      { values: [{ when: { brand: 'acme', mode: 'dark' }, expression: literalExpr(color('#123456')) }] })];
  } else if (scenario.kind === 'sets') {
    s.sets = [
      { id: ids.setA, name: 'base', when: {} },
      { id: ids.setB, name: 'brand', when: { brand: 'acme' } },
      { id: ids.setC, name: 'contrast', when: { contrast: 'high' } },
    ];
    s.activeSetIds = [ids.setA, ids.setB, ids.setC];
    s.tokens = [
      token(ids.a, 'base', 'color', literalExpr(color('#ffffff'))),
      token(ids.a, 'brand', 'color', literalExpr(color('#333333')), { setId: ids.setB }),
      token(ids.a, 'contrast', 'color', literalExpr(color('#000000')), { setId: ids.setC }),
    ];
  } else if (scenario.kind === 'expression') {
    s.tokens = [token(ids.a, 'space', 'dimension', { kind: 'operation', operator: 'add', arguments: [
      literalExpr(dimension(scenario.left.value, scenario.left.unit)), literalExpr(dimension(scenario.right.value, scenario.right.unit)),
    ] })];
  } else if (scenario.kind === 'expression-error') {
    s.tokens = [token(ids.a, 'space', 'dimension', { kind: 'operation', operator: scenario.operator, arguments: [
      literalExpr(dimension(1, scenario.leftUnit)), literalExpr(dimension(1, scenario.rightUnit)),
    ] })];
  } else if (scenario.kind === 'division-zero') {
    s.tokens = [token(ids.a, 'division', 'number', { kind: 'operation', operator: 'divide', arguments: [literalExpr(number(1)), literalExpr(number(0))] })];
  } else if (scenario.kind === 'expression-node-budget') {
    const tree = (depth) => depth === 0 ? literalExpr(number(1)) : { kind: 'operation', operator: 'add', arguments: [tree(depth - 1), tree(depth - 1)] };
    s.tokens = [token(ids.a, 'large-expression', 'number', tree(scenario.depth))];
  } else if (scenario.kind === 'rename') {
    s.tokens = [token(scenario.tokenId, scenario.afterName, 'color', literalExpr(color('#16a34a')))];
  }
  return s;
}

function requestForVector(vector) {
  const scenario = vector.scenario;
  const fallback = scenario.fallbackHex ? color(scenario.fallbackHex) : color('#6b7280');
  if (scenario.kind === 'direct') return request({ kind: 'literal', value: color(scenario.hex) }, 'color');
  if (scenario.kind === 'alias-chain' || scenario.kind === 'rename') return request({ kind: 'binding', target: { namespace: ns, tokenId: scenario.kind === 'rename' ? scenario.tokenId : ids.a }, modeStrategy: { kind: 'inherit' }, fallback }, 'color');
  if (scenario.kind === 'alias-budget') return request({ kind: 'binding', target: { namespace: ns, tokenId: '00000000-0000-4000-8000-000000000001' }, modeStrategy: { kind: 'inherit' } }, 'color');
  if (scenario.kind === 'missing-fallback') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' }, fallback }, 'color');
  if (scenario.kind === 'missing-no-fallback') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' } }, 'color');
  if (scenario.kind === 'context-budget') {
    const explicit = Object.fromEntries(Array.from({ length: 8 }, (_, index) => [`dimension-${index}`, `v${index}`]));
    return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' } }, 'color', context({ explicit, document: { ninth: 'value' } }));
  }
  if (scenario.kind === 'alias-cycle') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' }, fallback }, 'color');
  if (scenario.kind === 'modes') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' }, fallback }, 'color', context({ ancestors: [{ mode: scenario.context }] }));
  if (scenario.kind === 'dimensions') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' }, fallback }, 'color', context({ explicit: scenario.explicit, ancestors: [scenario.nearestAncestor], page: scenario.page }));
  if (scenario.kind === 'sets') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' }, fallback }, 'color', context({ explicit: { brand: 'acme', contrast: 'high' } }));
  if (scenario.kind === 'expression') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' }, fallback: dimension(0, 'px') }, 'dimension');
  if (scenario.kind === 'expression-error') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' } }, 'dimension');
  if (scenario.kind === 'division-zero' || scenario.kind === 'expression-node-budget') return request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' } }, 'number');
  return null;
}

const expectedVectorIds = new Set([
  'direct-no-tokens', 'create-token-from-property', 'apply-override-revert-detach', 'multi-hop-alias',
  'missing-reference-fallback', 'missing-reference-without-fallback', 'circular-alias', 'alias-budget-exhaustion', 'mode-inheritance-and-explicit',
  'multiple-dimension-precedence', 'ordered-set-precedence', 'formula-evaluation', 'formula-type-error',
  'formula-division-by-zero', 'formula-budget-exhaustion', 'expression-node-budget', 'context-dimension-budget', 'rename-preserves-id-binding', 'unknown-extension-round-trip',
  'repeat-resolution-deterministic', 'agent-proposal-accepted', 'agent-proposal-rejected',
]);

test('proposed golden fixture is versioned, structurally valid, and complete', () => {
  assert.equal(validateFixture(golden), true, JSON.stringify(validateFixture.errors));
  assert.deepEqual(new Set(golden.vectors.map((v) => v.id)), expectedVectorIds);
  assert.ok(golden.vectors.every((v) => v.status === 'PROPOSED'));
});

test('full v0.1 request/result fixtures validate and match the JavaScript oracle', () => {
  assert.equal(validateResolutionVectors(resolutionVectors), true, JSON.stringify(validateResolutionVectors.errors));
  for (const vector of resolutionVectors.cases) {
    assert.deepEqual(resolve(vector.input.snapshot, vector.input.request), vector.expectedOutput, `fixture ${vector.id}`);
  }
});

for (const vector of golden.vectors) {
  test(`PROPOSED vector: ${vector.id}`, () => {
    const scenario = vector.scenario;
    if (['direct', 'alias-chain', 'alias-budget', 'missing-fallback', 'missing-no-fallback', 'context-budget', 'alias-cycle', 'modes', 'dimensions', 'sets', 'expression', 'expression-error', 'division-zero', 'expression-node-budget', 'rename'].includes(scenario.kind)) {
      const input = { snapshot: snapshotForVector(vector), request: requestForVector(vector) };
      assert.equal(validateSnapshot(input.snapshot), true, JSON.stringify(validateSnapshot.errors));
      assert.equal(validateRequest(input.request), true, JSON.stringify(validateRequest.errors));
      const before = JSON.stringify(input);
      const result = resolve(input.snapshot, input.request);
      assert.equal(validateResult(result), true, JSON.stringify(validateResult.errors));
      assert.equal(JSON.stringify(input), before, 'resolution must not mutate its inputs');
      if (vector.expected.status) assert.equal(result.status, vector.expected.status);
      if (vector.expected.source) assert.equal(result.source, vector.expected.source);
      if (vector.expected.type) assert.equal(result.value.type, vector.expected.type);
      if (vector.expected.traceLength !== undefined) assert.equal(result.dependencyTrace.length, vector.expected.traceLength);
      if (vector.expected.diagnostic) assert.equal(result.diagnostics[0]?.code, vector.expected.diagnostic);
      if (vector.expected.hasValue === false) assert.equal(Object.hasOwn(result, 'value'), false);
      if (vector.id === 'direct-no-tokens') assert.equal(result.value.value.hex, scenario.hex);
      if (vector.id === 'formula-evaluation') assert.deepEqual(result.value, dimension(vector.expected.value.value, vector.expected.value.unit));
      if (vector.id === 'multiple-dimension-precedence') assert.deepEqual(result.effectiveContext, vector.expected.effective);
      if (vector.id === 'ordered-set-precedence') assert.equal(result.value.value.hex, vector.expected.resolved);
      if (vector.id === 'rename-preserves-id-binding') {
        assert.equal(input.request.property.target.tokenId, scenario.tokenId);
        assert.equal(result.status, vector.expected.status);
      }
      if (vector.id === 'mode-inheritance-and-explicit') {
        const explicit = { ...input.request, property: { ...input.request.property, modeStrategy: { kind: 'explicit', modeId: ids.light } } };
        assert.equal(resolve(input.snapshot, explicit).value.value.hex, vector.expected.explicit);
        assert.equal(result.value.value.hex, vector.expected.inherited);
      }
    } else if (scenario.kind === 'expression-budget') {
      let expression = literalExpr(number(1));
      for (let i = 0; i < scenario.depth; i += 1) expression = { kind: 'operation', operator: 'add', arguments: [expression, literalExpr(number(1))] };
      const s = emptySnapshot();
      s.tokens = [token(ids.a, 'budget', 'number', expression)];
      const result = resolve(s, request({ kind: 'binding', target: { namespace: ns, tokenId: ids.a }, modeStrategy: { kind: 'inherit' } }, 'number'));
      assert.equal(result.status, vector.expected.status);
      assert.equal(result.diagnostics[0].code, vector.expected.diagnostic);
    } else if (scenario.kind === 'create-and-bind') {
      assert.equal(scenario.initial, 'literal');
      assert.equal(scenario.operationCount, 2);
      assert.equal(vector.expected.atomic, true);
      assert.equal(vector.expected.requiresUserApproval, true);
    } else if (scenario.kind === 'binding-edits') {
      const target = { namespace: ns, tokenId: ids.a };
      const s = emptySnapshot();
      s.tokens = [token(ids.a, 'color/primary', 'color', literalExpr(color('#2563eb')))];
      let property = { kind: 'binding', target, modeStrategy: { kind: 'inherit' }, fallback: color('#2563eb') };
      const baseRequest = request(property, 'color');
      assert.equal(resolve(s, baseRequest).value.value.hex, '#2563eb');
      property = { ...property, localOverride: color('#ef4444') };
      assert.equal(property.target.tokenId, ids.a);
      assert.equal(Boolean(property.target), vector.expected.referenceAfterOverride);
      const overrideResult = resolve(s, request(property, 'color'));
      assert.equal(overrideResult.source, 'LOCAL_OVERRIDE');
      assert.equal(overrideResult.value.value.hex, '#ef4444');
      const reverted = { ...property };
      delete reverted.localOverride;
      assert.equal(Boolean(reverted.target), vector.expected.referenceAfterRevert);
      assert.equal(resolve(s, request(reverted, 'color')).source, 'TOKEN');
      const detached = { kind: 'literal', value: color('#2563eb') };
      assert.equal(detached.kind, vector.expected.finalKind);
      assert.equal(Boolean(detached.target), vector.expected.referenceAfterDetach);
      assert.equal(resolve(s, request(detached, 'color')).value.value.hex, '#2563eb');
    } else if (scenario.kind === 'extension') {
      const extension = { [scenario.key]: scenario.payload };
      assert.deepEqual(JSON.parse(JSON.stringify(extension)), extension);
      const bad = { ...token(ids.a, 'x', 'color', literalExpr(color('#000000'))), unregisteredField: true };
      assert.equal(validateSnapshot({ ...emptySnapshot(), tokens: [bad] }), false);
      assert.equal(vector.expected.preserved, true);
      assert.equal(vector.expected.unknownKnownFieldAccepted, false);
    } else if (scenario.kind === 'repeat') {
      const s = snapshotForVector(golden.vectors.find((v) => v.id === 'multi-hop-alias'));
      const q = requestForVector(golden.vectors.find((v) => v.id === 'multi-hop-alias'));
      const before = JSON.stringify({ s, q });
      const results = Array.from({ length: scenario.runs }, () => resolve(s, q));
      assert.deepEqual(results[1], results[0]);
      assert.deepEqual(results[2], results[0]);
      assert.equal(JSON.stringify({ s, q }), before);
    } else if (scenario.kind === 'plan') {
      assert.ok(['READY', 'REJECTED'].includes(vector.expected.validation));
      assert.equal(vector.status, 'PROPOSED');
    }
  });
}

test('schema compiler rejects unknown known-object fields while allowing extensions', () => {
  const valid = emptySnapshot();
  valid.extensions = { 'vendor.example': { opaque: true } };
  assert.equal(validateSnapshot(valid), true, JSON.stringify(validateSnapshot.errors));
  const invalid = { ...valid, accidental: true };
  assert.equal(validateSnapshot(invalid), false);
});
