import type { EventEnvelope } from "../../domain/contracts";
import { subscribeBrowserEvents } from "./browser-store";
import { ensureDemoRun, replayEvents } from "./commands";
import { isTauriRuntime } from "./runtime";

export const EVENT_CHANNEL = "tiamat://events";

export interface EventBridgeSnapshot {
  runId: string;
  events: EventEnvelope[];
}

/**
 * Load durable events (creating the demo run if needed) and subscribe for live appends.
 * Safe to call again after a React remount — replay is ordered and de-duplicated by eventId.
 */
export async function connectEventBridge(
  onEvents: (events: EventEnvelope[]) => void,
): Promise<() => void> {
  const snapshot = await ensureDemoRun();
  const initial = await replayEvents(snapshot.run.runId, 0);
  onEvents(dedupeByEventId(initial));

  if (isTauriRuntime()) {
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<EventEnvelope>(EVENT_CHANNEL, (event) => {
      onEvents([event.payload]);
    });
    return () => {
      unlisten();
    };
  }

  return subscribeBrowserEvents((events) => onEvents(events));
}

export function mergeEvents(
  existing: EventEnvelope[],
  incoming: EventEnvelope[],
): EventEnvelope[] {
  if (incoming.length === 0) return existing;
  const seen = new Set<string>();
  const out: EventEnvelope[] = [];
  for (const event of existing) {
    if (seen.has(event.eventId)) continue;
    seen.add(event.eventId);
    out.push(event);
  }
  for (const event of incoming) {
    if (seen.has(event.eventId)) continue;
    seen.add(event.eventId);
    out.push(event);
  }
  // Incoming bursts are already ordered; only sort when needed.
  let needsSort = false;
  for (let i = 1; i < out.length; i += 1) {
    if (out[i]!.sequence < out[i - 1]!.sequence) {
      needsSort = true;
      break;
    }
  }
  if (needsSort) {
    out.sort((a, b) => a.sequence - b.sequence);
  }
  return out;
}

function dedupeByEventId(events: EventEnvelope[]): EventEnvelope[] {
  const seen = new Set<string>();
  const out: EventEnvelope[] = [];
  for (const event of events) {
    if (seen.has(event.eventId)) continue;
    seen.add(event.eventId);
    out.push(event);
  }
  return out.sort((a, b) => a.sequence - b.sequence);
}
