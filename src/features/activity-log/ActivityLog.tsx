import type { EventEnvelope } from "../../domain/contracts";
import {
  DEFAULT_EVENT_FILTER,
  exportFilteredEventsJson,
  filterEvents,
  truncateMessage,
  type EventFilter,
} from "../../domain/events";
import { useEffect, useMemo, useRef, useState } from "react";
import "./ActivityLog.css";

interface ActivityLogProps {
  events: EventEnvelope[];
  statusLine?: string;
}

const ROW_HEIGHT = 44;
const VIEWPORT_HEIGHT = 360;
const OVERSCAN = 6;
const MAX_MOUNTED_ROWS = 64;

export function ActivityLog({ events, statusLine }: ActivityLogProps) {
  const [filter, setFilter] = useState<EventFilter>(DEFAULT_EVENT_FILTER);
  const visible = useMemo(() => filterEvents(events, filter), [events, filter]);
  const [follow, setFollow] = useState(true);
  const [windowStart, setWindowStart] = useState(0);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const lastLenRef = useRef(0);

  const windowSize = Math.min(
    MAX_MOUNTED_ROWS,
    Math.ceil(VIEWPORT_HEIGHT / ROW_HEIGHT) + OVERSCAN * 2,
  );

  useEffect(() => {
    if (!follow) return;
    if (visible.length === lastLenRef.current && visible.length > 0) return;
    lastLenRef.current = visible.length;
    const start = Math.max(0, visible.length - windowSize);
    setWindowStart(start);
    const node = scrollerRef.current;
    if (node) {
      node.scrollTop = visible.length * ROW_HEIGHT;
    }
  }, [visible, follow, windowSize]);

  function onScroll() {
    const node = scrollerRef.current;
    if (!node) return;
    const start = Math.max(
      0,
      Math.floor(node.scrollTop / ROW_HEIGHT) - OVERSCAN,
    );
    setWindowStart(start);
    const nearBottom =
      node.scrollTop + node.clientHeight >= node.scrollHeight - ROW_HEIGHT * 2;
    if (!nearBottom && follow) setFollow(false);
    else if (nearBottom && !follow) setFollow(true);
  }

  const mounted = visible.slice(windowStart, windowStart + windowSize);

  function onExport() {
    const blob = new Blob([exportFilteredEventsJson(events, filter)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "tiamat-log.json";
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <section className="activity-log pane" data-testid="activity-log">
      <header className="pane-header log-header">
        <div>
          <h2>Log</h2>
          {statusLine && (
            <p className="status-line" data-testid="run-status-line">
              {statusLine}
            </p>
          )}
        </div>
        <div className="log-toolbar">
          <select
            data-testid="log-level-filter"
            aria-label="Filter by level"
            value={filter.level}
            onChange={(e) =>
              setFilter({
                ...filter,
                level: e.target.value as EventFilter["level"],
              })
            }
          >
            <option value="all">All levels</option>
            <option value="info">Info</option>
            <option value="warning">Warning</option>
            <option value="error">Error</option>
          </select>
          <input
            data-testid="log-search"
            aria-label="Search log"
            placeholder="Search…"
            value={filter.query}
            onChange={(e) => setFilter({ ...filter, query: e.target.value })}
          />
          <button type="button" onClick={() => setFollow(true)}>
            Follow
          </button>
          <button type="button" data-testid="log-export" onClick={onExport}>
            Export
          </button>
          <span data-testid="log-count">{visible.length}</span>
        </div>
      </header>

      <div
        className="log-scroller"
        ref={scrollerRef}
        onScroll={onScroll}
        style={{ height: VIEWPORT_HEIGHT }}
        data-testid="log-scroller"
      >
        <div style={{ height: visible.length * ROW_HEIGHT, position: "relative" }}>
          {mounted.map((event, idx) => {
            const top = (windowStart + idx) * ROW_HEIGHT;
            return (
              <div
                key={event.eventId}
                className={`log-row level-${event.level}`}
                data-testid="log-event"
                data-event-id={event.eventId}
                data-sequence={event.sequence}
                data-type={event.type}
                style={{
                  position: "absolute",
                  top,
                  left: 0,
                  right: 0,
                  height: ROW_HEIGHT,
                }}
              >
                <span className="log-seq">{event.sequence}</span>
                <span className="log-type">{event.type}</span>
                <span className="log-msg">
                  {truncateMessage(event.message, 180).text}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
