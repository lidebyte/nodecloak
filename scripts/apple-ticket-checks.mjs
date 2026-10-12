import test from 'node:test';
import assert from 'node:assert/strict';
import {validateAppleTicket, stapleAppleTicket} from './apple-ticket.mjs';

const lost = {status: 68, stdout: '', stderr: 'NSURLErrorDomain Code=-1005 "The network connection was lost."'};

test('ticket validation retries network loss and requires eventual success', async () => {
  const delays = [];
  let calls = 0;
  await validateAppleTicket('fixture.app', {
    execute(command, args) {
      assert.equal(command, 'xcrun');
      assert.deepEqual(args, ['stapler', 'validate', 'fixture.app']);
      return ++calls < 3 ? lost : {status: 0};
    },
    wait: async delay => delays.push(delay),
    log: () => {},
  });
  assert.equal(calls, 3);
  assert.deepEqual(delays, [5000, 10000]);
});

test('invalid tickets fail immediately, including non-network error 68', async () => {
  for (const result of [{status: 65, stderr: 'Ticket is invalid'}, {status: 68, stderr: 'CloudKit record not found'}]) {
    let calls = 0;
    await assert.rejects(validateAppleTicket('fixture.app', {
      execute: () => { calls++; return result; },
      wait: () => assert.fail('Invalid tickets must not be retried'),
    }), /failed/);
    assert.equal(calls, 1);
  }
});

test('persistent network failure exhausts bounded retries and blocks release', async () => {
  let calls = 0;
  const delays = [];
  await assert.rejects(validateAppleTicket('fixture.app', {
    execute: () => { calls++; return lost; },
    wait: async delay => delays.push(delay),
    log: () => {},
  }), /network connection was lost/);
  assert.equal(calls, 4);
  assert.deepEqual(delays, [5000, 10000, 20000]);
});

test('stapling also retries network interruption and must complete successfully', async () => {
  let calls = 0;
  const delays = [];
  await stapleAppleTicket('fixture.app', {
    execute(command, args) {
      assert.equal(command, 'xcrun');
      assert.deepEqual(args, ['stapler', 'staple', 'fixture.app']);
      return ++calls === 1 ? lost : {status: 0};
    },
    wait: async delay => delays.push(delay),
    log: () => {},
  });
  assert.equal(calls, 2);
  assert.deepEqual(delays, [5000]);
});
