import assert from 'node:assert/strict';
import test from 'node:test';
import { createConcurrencyLimiter, withTimeout } from '../src/utils/promise.ts';

test('limiter completes jobs without exceeding capacity', async () => {
  const limit = createConcurrencyLimiter(2);
  let active = 0;
  let peak = 0;
  const result = await Promise.all([1, 2, 3, 4].map(value => limit(async () => {
    active++;
    peak = Math.max(peak, active);
    await new Promise(resolve => setTimeout(resolve, 1));
    active--;
    return value;
  })));
  assert.deepEqual(result, [1, 2, 3, 4]);
  assert.equal(peak, 2);
});

test('timeout wrapper retains successful promise values', async () => {
  assert.equal(await withTimeout(Promise.resolve(17), 100), 17);
});
