import type { EventEnvelope } from "../../domain/contracts";
import {
  DEFAULT_EVENT_FILTER,
  EVENT_CATEGORIES,
  exportFilteredEventsJson,
  filterEvents,
  truncateMessage,
  type EventFilter,
} from "../../domain/events";
import { useEffect, useMemo, useRef, useState } from "react";
import "./ActivityLog.css";

interface ActivityLogProps {
  events: EventEnvelope[];
  filter: EventFilter;
  onFilterChange: (filter: EventFilter) => void;
  selectedPhaseId?: string | null;
  onExportReport?: () => void;
}

const ROW_HEIGHT = 44;
const VIEWPORT_HEIGHT = 280;
const OVERSCAN = 6;
/** Hard bound on mounted log rows (virtualization window). */
const MAX_MOUNTED_ROWS = 64;

export function ActivityLog({
  events,
  filter,
  onFilterChange,
  selectedPhaseId = null,
  onExportReport,
}: ActivityLogProps) {
  const visible = useMemo(() => filterEvents(events, filter), [events, filter]);
  const [follow, setFollow] = useState(true);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set());
  const [windowStart, setWindowStart] = useState(0);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const lastLenRef = useRef(0);

  useEffect(() => {
    if (selectedPhaseId && filter.phaseId !== selectedPhaseId) {
      onFilterChange({ ...filter, phaseId: selectedPhaseId });
    }
  }, [selectedPhaseId]); // eslint-disable-line react-hooks/exhaustive-deps

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

  function toggleExpand(eventId: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(eventId)) next.delete(eventId);
      else next.add(eventId);
      return next;
    });
  }

  function onExportLog() {
    const json = exportFilteredEventsJson(events, filter);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `tiamat-log-${Date.now()}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  return (
    <section className="tiamat-panel tiamat-log" aria-label="Activity log">
      <div className="tiamat-log-header">
        <h2>Activity log</h2>
        <div className="tiamat-log-actions">
          <label className="tiamat-log-follow">
            <input
              type="checkbox"
              data-testid="log-follow"
              checked={follow}
              onChange={(event) => setFollow(event.target.checked)}
            />
            Follow
          </label>
          <button type="button" data-testid="log-export" onClick={onExportLog}>
            Export log
          </button>
          {onExportReport ? (
            <button
              type="button"
              data-testid="report-export"
              onClick={onExportReport}
            >
              Export report
            </button>
          ) : null}
        </div>
      </div>
      <div className="tiamat-log-filters" data-testid="log-filters">
        <label>
          Search
          <input
            data-testid="log-search"
            value={filter.query}
            onChange={(event) =>
              onFilterChange({ ...filter, query: event.target.value })
            }
            placeholder="message, type, phase…"
          />
        </label>
        <label>
          Level
          <select
            data-testid="log-level"
            value={filter.level}
            onChange={(event) =>
              onFilterChange({
                ...filter,
                level: event.target.value as EventFilter["level"],
              })
            }
          >
            <option value="all">all</option>
            <option value="debug">debug</option>
            <option value="info">info</option>
            <option value="warning">warning</option>
            <option value="error">error</option>
          </select>
        </label>
        <label>
          Category
          <select
            data-testid="log-category"
            value={filter.category}
            onChange={(event) =>
              onFilterChange({
                ...filter,
                category: event.target.value as EventFilter["category"],
              })
            }
          >
            {EVENT_CATEGORIES.map((category) => (
              <option key={category} value={category}>
                {category}
              </option>
            ))}
          </select>
        </label>
        <label>
          Type
          <input
            data-testid="log-type"
            value={filter.typePrefix}
            onChange={(event) =>
              onFilterChange({ ...filter, typePrefix: event.target.value })
            }
            placeholder="phase."
          />
        </label>
        <label>
          Phase
          <input
            data-testid="log-phase"
            value={filter.phaseId}
            onChange={(event) =>
              onFilterChange({ ...filter, phaseId: event.target.value })
            }
            placeholder="P01"
          />
        </label>
        <label>
          Attempt
          <input
            data-testid="log-attempt"
            value={filter.attemptId}
            onChange={(event) =>
              onFilterChange({ ...filter, attemptId: event.target.value })
            }
            placeholder="attempt id"
          />
        </label>
        <label>
          Project
          <input
            data-testid="log-project"
            value={filter.projectId}
            onChange={(event) =>
              onFilterChange({ ...filter, projectId: event.target.value })
            }
            placeholder="project id"
          />
        </label>
      </div>
      <div
        className="tiamat-log-virtual"
        data-testid="activity-log"
        data-virtualized="true"
        data-visible-bound={MAX_MOUNTED_ROWS}
      >
        {visible.length === 0 ? (
          <p className="tiamat-muted" data-testid="log-empty">
            No events match the current filters.
          </p>
        ) : (
          <div
            ref={scrollerRef}
            className="tiamat-log-scroller"
            data-testid="log-vlist"
            style={{ height: VIEWPORT_HEIGHT, overflow: "auto" }}
            onScroll={onScroll}
          >
            <div
              className="tiamat-log-spacer"
              style={{ height: visible.length * ROW_HEIGHT, position: "relative" }}
            >
              {mounted.map((event, index) => {
                const truncated = truncateMessage(event.message);
                const expanded = expandedIds.has(event.eventId);
                const showFull = expanded || !truncated.truncated;
                const top = (windowStart + index) * ROW_HEIGHT;
                return (
                  <div
                    key={event.eventId}
                    className="tiamat-log-row"
                    data-testid="log-event"
                    data-sequence={event.sequence}
                    data-event-id={event.eventId}
                    data-type={event.type}
                    data-level={event.level}
                    data-phase-id={event.phaseId ?? ""}
                    style={{
                      position: "absolute",
                      top,
                      left: 0,
                      right: 0,
                      height: ROW_HEIGHT,
                    }}
                  >
                    <span className="tiamat-log-seq">#{event.sequence}</span>
                    <span className={`tiamat-log-level level-${event.level}`}>
                      {event.level}
                    </span>
                    <span className="tiamat-log-type">{event.type}</span>
                    <span
                      className="tiamat-log-message"
                      title={event.message}
                      data-truncated={
                        truncated.truncated && !expanded ? "true" : "false"
                      }
                    >
                      {showFull ? event.message : truncated.text}
                    </span>
                    {truncated.truncated ? (
                      <button
                        type="button"
                        className="tiamat-log-expand"
                        data-testid="log-expand"
                        onClick={() => toggleExpand(event.eventId)}
                      >
                        {expanded ? "Collapse" : "Expand"}
                      </button>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
      <p className="tiamat-muted" data-testid="log-count">
        Showing {visible.length} of {events.length} persisted events
        {follow ? " · following" : " · follow paused"}
      </p>
    </section>
  );
}

export { DEFAULT_EVENT_FILTER };
