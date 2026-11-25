// A Cloud chat whose machine is paused rests (cmux#57136: "this has reconnecting very often for some
// reason… are we not pausing this vm when the agent isn't active?"). cmux Cloud pauses an idle machine
// and its chat's link ends with it. Before, the pane treated that as a lost link: it asked again every
// 0.5-2 s for two minutes saying "Reconnecting…", then showed a failure line, and asked again every
// time the window was focused. Now the pane keeps the chat it shows, says
// nothing, never asks on a timer or a focus, and asks again once the machine wakes (a send, a
// keystroke, a terminal or a resume elsewhere).
//
// These run the loop's rules on a simulated clock, as agent-pane-reconnect-bound.test.ts does.
import { expect, test } from "bun:test";
import { cloudChatAsleep, cloudSleepingMachine, cloudWakeAsksAgain } from "../src/agent-session/acpmux/cloudSleep";
import {
  hostRetryStep,
  hostWakeAsks,
  lostRetryStep,
  nextRetryDelay,
  RETRY_FIRST_MS,
} from "../src/agent-session/acpmux/hostRetry";

const HOUR_MS = 3_600_000;
/// The pane's handshake to a paused Cloud machine: the app has no link to it.
const notConnected = Object.assign(new Error("not connected to cmux-tui"), { code: "native.not_connected" });
const paused = { machine: "vm_843faedb86db82ff2333", link: "paused", words: "Paused" };
const resuming = { machine: "vm_843faedb86db82ff2333", link: "connecting", words: "Resuming…" };

/// The handshakes an hour of a paused machine takes, as the pane's catch block schedules them, and
/// how many of them the window's focus asks for (`focuses` times an hour, as a person switching back).
function asksWhilePaused(asleep: boolean, focuses = 30): number {
  let now = 0;
  let delay = RETRY_FIRST_MS;
  let asks = 0;
  let stopped = false;
  const focusEvery = HOUR_MS / focuses;
  let nextFocus = focusEvery;
  while (now < HOUR_MS) {
    if (!stopped) {
      asks += 1;
      const step = hostRetryStep({ error: notConnected, troubleForMs: now, waitMs: delay, asleep });
      if (step.kind !== "retry") {
        stopped = true;
        continue;
      }
      now += step.waitMs;
      delay = nextRetryDelay(delay, true);
      continue;
    }
    // A stopped loop asks once more on each focus of the window.
    now = nextFocus;
    nextFocus += focusEvery;
    if (now >= HOUR_MS) break;
    if (hostWakeAsks({ event: "focus", stopped, connecting: false, hidden: false, answerSpent: false, asleep })) {
      stopped = false;
      delay = RETRY_FIRST_MS;
    }
  }
  return asks;
}

test("a paused Cloud machine's chat stops asking at once and a focus does not ask it again", () => {
  const before = asksWhilePaused(false);
  const after = asksWhilePaused(true);
  console.log(`handshakes an hour on a paused Cloud machine: before ${before}, after ${after}`);
  // Before: two minutes of quick asks (about 64), then one more failed ask on every focus.
  expect(before).toBeGreaterThan(90);
  // After: the one ask that found the machine paused.
  expect(after).toBe(1);
});

test("the loop rests on a paused machine: no timer after a failed handshake or a lost link", () => {
  expect(hostRetryStep({ error: notConnected, troubleForMs: 0, waitMs: 250, asleep: true })).toEqual({ kind: "rest" });
  expect(lostRetryStep({ troubleForMs: 0, upForMs: 600_000, waitMs: 250, asleep: true })).toEqual({ kind: "rest" });
  // A machine that is up keeps the recovery it had.
  expect(hostRetryStep({ error: notConnected, troubleForMs: 0, waitMs: 250, asleep: false }).kind).toBe("retry");
  expect(lostRetryStep({ troubleForMs: 0, upForMs: 600_000, waitMs: 250 }).kind).toBe("retry");
  // A chat too large to open still stops (only Retry asks), asleep or not.
  expect(hostRetryStep({ error: { reason: "message.too_large" }, troubleForMs: 0, waitMs: 250, asleep: true })).toEqual(
    {
      kind: "stop",
    },
  );
});

test("the app's link word says the machine sleeps; it wins over the recovery's older read", () => {
  expect(cloudChatAsleep({ link: paused, recovery: undefined })).toBe(true);
  // Woken elsewhere: the app's link reads it on its way, so the pane never rests on a running machine.
  expect(cloudChatAsleep({ link: resuming, recovery: "paused" })).toBe(false);
  // A chat on this Mac (no link word): only the recovery's read says.
  expect(cloudChatAsleep({ link: null, recovery: "paused" })).toBe(true);
  expect(cloudChatAsleep({ link: null, recovery: "live" })).toBe(false);
  expect(cloudChatAsleep({ link: null, recovery: undefined })).toBe(false);
});

test("a send wakes the machine the app reads paused, and its wake asks the pane to connect", () => {
  // A chat opened while its machine already slept has no handshake identity: the link names it.
  expect(cloudSleepingMachine(paused)).toBe("vm_843faedb86db82ff2333");
  expect(cloudSleepingMachine(resuming)).toBeUndefined();
  expect(cloudSleepingMachine(null)).toBeUndefined();
  // The machine stops reading asleep: the pane asks again, once, unless a client is already connected.
  expect(cloudWakeAsksAgain({ wasAsleep: true, asleep: false, connected: false })).toBe(true);
  expect(cloudWakeAsksAgain({ wasAsleep: true, asleep: false, connected: true })).toBe(false);
  expect(cloudWakeAsksAgain({ wasAsleep: false, asleep: false, connected: false })).toBe(false);
  expect(cloudWakeAsksAgain({ wasAsleep: false, asleep: true, connected: false })).toBe(false);
});
