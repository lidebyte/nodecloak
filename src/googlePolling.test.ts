import {afterEach, describe, expect, it, vi} from 'vitest';
import {startGooglePolling} from './googlePolling';

afterEach(() => vi.useRealTimers());

describe('Google sign-in polling', () => {
 it('handles completion when the request changes the controller busy state', async () => {
  vi.useFakeTimers();
  let busy = false;
  let complete!: (result: {pending?: boolean}) => void;
  const finished = vi.fn();
  const poll = vi.fn(() => {
   busy = true;
   return new Promise<{pending?: boolean}>(resolve => {complete = resolve;});
  });
  const stop = startGooglePolling({isBusy: () => busy, poll, onFinish: finished});
  await vi.advanceTimersByTimeAsync(2000);
  await vi.advanceTimersByTimeAsync(4000);
  expect(poll).toHaveBeenCalledTimes(1);
  busy = false;
  complete({});
  await vi.advanceTimersByTimeAsync(0);
  expect(finished).toHaveBeenCalledWith(true);
  await vi.advanceTimersByTimeAsync(6000);
  expect(poll).toHaveBeenCalledTimes(1);
  stop();
 });

 it('continues pending requests, skips other actions, and stops on an error result', async () => {
  vi.useFakeTimers();
  let busy = true;
  const poll = vi.fn().mockResolvedValueOnce({pending: true}).mockResolvedValueOnce(null);
  const finished = vi.fn();
  startGooglePolling({isBusy: () => busy, poll, onFinish: finished});
  await vi.advanceTimersByTimeAsync(2000);
  expect(poll).not.toHaveBeenCalled();
  busy = false;
  await vi.advanceTimersByTimeAsync(2000);
  expect(finished).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(2000);
  expect(finished).toHaveBeenCalledWith(false);
  await vi.advanceTimersByTimeAsync(6000);
  expect(poll).toHaveBeenCalledTimes(2);
 });

 it('ignores an outstanding response after the polling owner unmounts', async () => {
  vi.useFakeTimers();
  let complete!: (result: {pending?: boolean}) => void;
  const finished = vi.fn();
  const stop = startGooglePolling({isBusy: () => false, poll: () => new Promise(resolve => {complete = resolve;}), onFinish: finished});
  await vi.advanceTimersByTimeAsync(2000);
  stop();
  complete({});
  await vi.advanceTimersByTimeAsync(6000);
  expect(finished).not.toHaveBeenCalled();
 });
});
