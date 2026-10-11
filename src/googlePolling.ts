type GooglePollResult = {pending?: boolean} | null;

export function startGooglePolling({isBusy, poll, onFinish}: {
 isBusy: () => boolean;
 poll: () => Promise<GooglePollResult>;
 onFinish: (succeeded: boolean) => void;
}): () => void {
 let stopped = false;
 let inFlight = false;
 const stop = () => {stopped = true; clearInterval(timer);};
 const timer = setInterval(async () => {
  if (stopped || inFlight || isBusy()) return;
  inFlight = true;
  try {
   const result = await poll();
   if (stopped || result?.pending) return;
   stop();
   onFinish(result !== null);
  } catch {
   if (!stopped) {stop(); onFinish(false);}
  } finally {
   inFlight = false;
  }
 }, 2000);
 return stop;
}
