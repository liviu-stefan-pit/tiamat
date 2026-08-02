import type { EventEnvelope, EventLevel } from "./contracts";

export type EventCategory =
  | "all"
  | "run"
  | "project"
  | "phase"
  | "attempt"
  | "agent"
  | "test"
  | "stdout"
  | "stderr"
  | "system";

export type EventFilter = {
  query: string;
  level: EventLevel | "all";
  typePrefix: string;
  category: EventCategory;
  phaseId: string;
  attemptId: string;
  projectId: string;
};

export const DEFAULT_EVENT_FILTER: EventFilter = {
  query: "",
  level: "all",
  typePrefix: "",
  category: "all",
  phaseId: "",
  attemptId: "",
  projectId: "",
};

export const EVENT_CATEGORIES: EventCategory[] = [
  "all",
  "run",
  "project",
  "phase",
  "attempt",
  "agent",
  "test",
  "stdout",
  "stderr",
  "system",
];

export function eventCategory(event: EventEnvelope): EventCategory {
  const type = event.type.toLowerCase();
  if (type.startsWith("run.") || type.startsWith("scheduler.")) return "run";
  if (type.startsWith("project.") || type.startsWith("intake.")) return "project";
  if (type.startsWith("phase.")) return "phase";
  if (type.startsWith("attempt.")) return "attempt";
  if (type.startsWith("agent.") || type.startsWith("cursor.")) return "agent";
  if (type.startsWith("test.") || type.startsWith("evidence.")) return "test";
  if (type.includes("stdout") || type.endsWith(".stdout")) return "stdout";
  if (type.includes("stderr") || type.endsWith(".stderr")) return "stderr";
  if (type.startsWith("system.") || type.startsWith("cleanup.") || type.startsWith("watchdog.")) {
    return "system";
  }
  return "system";
}

export function filterEvents(
  events: EventEnvelope[],
  filter: EventFilter,
): EventEnvelope[] {
  const query = filter.query.trim().toLowerCase();
  const typePrefix = filter.typePrefix.trim().toLowerCase();
  const phaseId = filter.phaseId.trim().toLowerCase();
  const attemptId = filter.attemptId.trim().toLowerCase();
  const projectId = filter.projectId.trim().toLowerCase();

  return events.filter((event) => {
    if (filter.level !== "all" && event.level !== filter.level) {
      return false;
    }
    if (typePrefix && !event.type.toLowerCase().startsWith(typePrefix)) {
      return false;
    }
    if (filter.category !== "all" && eventCategory(event) !== filter.category) {
      return false;
    }
    if (phaseId && (event.phaseId ?? "").toLowerCase() !== phaseId) {
      return false;
    }
    if (attemptId && (event.attemptId ?? "").toLowerCase() !== attemptId) {
      return false;
    }
    if (projectId && (event.projectId ?? "").toLowerCase() !== projectId) {
      return false;
    }
    if (!query) {
      return true;
    }
    const haystack = [
      event.message,
      event.type,
      event.phaseId ?? "",
      event.projectId ?? "",
      event.attemptId ?? "",
      String(event.sequence),
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(query);
  });
}

export const LOG_MESSAGE_TRUNCATE_AT = 160;

export function truncateMessage(
  message: string,
  limit = LOG_MESSAGE_TRUNCATE_AT,
): { text: string; truncated: boolean } {
  if (message.length <= limit) {
    return { text: message, truncated: false };
  }
  return { text: `${message.slice(0, limit)}…`, truncated: true };
}

export function exportFilteredEventsJson(
  events: EventEnvelope[],
  filter: EventFilter,
): string {
  const filtered = filterEvents(events, filter).map((event) => ({
    sequence: event.sequence,
    eventId: event.eventId,
    runId: event.runId,
    type: event.type,
    level: event.level,
    timestampUtc: event.timestampUtc,
    phaseId: event.phaseId ?? null,
    attemptId: event.attemptId ?? null,
    projectId: event.projectId ?? null,
    message: event.message,
    category: eventCategory(event),
  }));
  return JSON.stringify(
    {
      schemaVersion: 1,
      exportedAtUtc: new Date().toISOString(),
      filter,
      count: filtered.length,
      events: filtered,
    },
    null,
    2,
  );
}
