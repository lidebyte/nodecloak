import {spawnSync} from 'node:child_process';
import {setTimeout} from 'node:timers/promises';
import {projectRoot} from './release-utils.mjs';

export async function validateAppleTicket(file, {execute = spawnSync, wait = setTimeout, log = console.warn} = {}) {
  const delays = [5000, 10000, 20000];
  for (let attempt = 0; ; attempt++) {
    const result = execute('xcrun', ['stapler', 'validate', file], {cwd: projectRoot, encoding: 'utf8'});
    if (result.error) throw new Error('Unable to run Apple ticket validation');
    if (result.status === 0) return;
    const output = [result.stderr, result.stdout].filter(Boolean).join('\n');
    // Retry only the observed CloudKit transport interruption, never an invalid ticket.
    if (result.status !== 68 || !/NSURLErrorDomain Code=-1005\b/.test(output) || attempt >= delays.length) {
      throw new Error(`Apple ticket validation failed (${result.status}): ${output}`);
    }
    log(`Apple ticket validation connection interrupted; retry ${attempt + 1}/${delays.length} in ${delays[attempt] / 1000}s.`);
    await wait(delays[attempt]);
  }
}
