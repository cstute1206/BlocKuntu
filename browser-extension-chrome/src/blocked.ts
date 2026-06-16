"use strict";

(() => {
type ReasonData = Record<string, unknown>;

const params = new URLSearchParams(location.search);
const url = params.get("url") || "unknown";
const reasonData = readReason();
const reason = stringValue("kind") || params.get("reason") || "blocked";
const message = stringValue("message") || params.get("message");
const ruleName = stringValue("rule_name") || params.get("rule_name");
const ruleId = stringValue("rule_id") || params.get("rule_id");
const tier = stringValue("tier") || params.get("tier");
const blockedBy = stringValue("blocked_by") || params.get("blocked_by");
const controlledReason = stringValue("controlled_reason") || params.get("controlled_reason");
const detoxSessionName = stringValue("session_name");
const detoxSessionId = stringValue("session_id");
const targetKind = stringValue("target_kind");
const freeAt = stringValue("free_at") || params.get("free_at");
const allowanceResetAt = stringValue("allowance_reset_at") || params.get("allowance_reset_at");
const lastHeartbeatOkAt = stringValue("last_heartbeat_ok_at") || params.get("last_heartbeat_ok_at");
const activeSchedules = arrayValue("active_schedules");

setText("blocked-url", url);
setText("blocked-reason", reasonTitle());
setText("summary", summaryText());
setText("detail", detailText());

if (tier) {
  addDetail("Tier", tierTitle(tier));
}
if (ruleName || ruleId) {
  addDetail("List", ruleName ? `${ruleName}${ruleId ? ` (${ruleId})` : ""}` : ruleId);
}
if (activeSchedules.length > 0) {
  addDetail("Schedule", scheduleText(activeSchedules));
}
if (reason === "detox" && (detoxSessionName || detoxSessionId)) {
  addDetail(
    "Detox session",
    detoxSessionName ? `${detoxSessionName}${detoxSessionId ? ` (${detoxSessionId})` : ""}` : detoxSessionId
  );
}
if (reason === "detox" && targetKind) {
  addDetail("Target", humanize(targetKind));
}
if (freeAt) {
  addDetail("Expected release", formatDateWithDistance(freeAt));
} else if (reason === "hard_block") {
  addDetail("Expected release", "No automatic release; Tier 1 is always blocked.");
} else if (reason === "backend_unhealthy" || reason === "backend_unavailable") {
  addDetail("Expected release", "When the daemon heartbeat is healthy again.");
} else if (reason === "controlled_access") {
  addDetail("Expected release", "When an unlock is active or this list becomes inactive.");
}
if (allowanceResetAt) {
  addDetail("Allowance reset", formatDateWithDistance(allowanceResetAt));
}
if (lastHeartbeatOkAt) {
  addDetail("Last heartbeat", formatDateWithDistance(lastHeartbeatOkAt));
}
addDetail("Technical reason", technicalReason());

function readReason(): ReasonData {
  const raw = params.get("reason_json");
  if (raw) {
    try {
      const parsed: unknown = JSON.parse(raw);
      if (isObject(parsed)) {
        return parsed;
      }
    } catch {
      // Fall back to legacy query parameters below.
    }
  }

  return {
    kind: params.get("reason") || "blocked",
    message: params.get("message") || undefined,
    rule_name: params.get("rule_name") || undefined,
    rule_id: params.get("rule_id") || undefined,
    controlled_reason: params.get("controlled_reason") || undefined,
  };
}

function stringValue(key: string): string | null {
  const value = reasonData[key];
  return typeof value === "string" && value.length > 0 ? value : null;
}

function arrayValue(key: string): unknown[] {
  const value = reasonData[key];
  return Array.isArray(value) ? value : [];
}

function reasonTitle(): string {
  if (reason === "detox") {
    return "Detox block";
  }
  if (reason === "hard_block") {
    return "Tier 1 hard block";
  }
  if (reason === "controlled_access") {
    if (blockedBy === "schedule") {
      return "Tier 2 scheduled block";
    }
    return "Tier 2 controlled access";
  }
  if (reason === "backend_unhealthy") {
    return "Daemon heartbeat missing";
  }
  if (reason === "backend_unavailable") {
    return "Daemon unavailable";
  }
  if (reason === "invalid_url") {
    return "Invalid URL";
  }
  if (reason === "runtime_error") {
    return "Daemon runtime error";
  }
  return humanize(reason);
}

function summaryText(): string {
  const summary = stringValue("summary");
  if (summary) {
    return summary;
  }
  if (message) {
    return message;
  }
  if (ruleName) {
    return `This navigation matched the list "${ruleName}".`;
  }
  if (reason === "detox") {
    return "Detox is active for this target.";
  }
  return "This navigation was blocked by the local policy.";
}

function detailText(): string {
  const detail = stringValue("detail");
  if (detail) {
    return detail;
  }
  if (reason === "backend_unhealthy" || reason === "backend_unavailable") {
    return "Browsing is blocked fail-closed until the Chrome extension, native host, and daemon can confirm policy enforcement.";
  }
  if (reason === "detox") {
    return "This temporary block stays active until the detox session ends or is cancelled from the privileged admin path.";
  }
  if (controlledReason === "allowance_exhausted") {
    return "The daily allowance for this list has been consumed.";
  }
  if (controlledReason === "no_allowance") {
    return "This list has no allowance configured, so it needs a policy-approved unlock.";
  }
  return "";
}

function scheduleText(schedules: unknown[]): string {
  return schedules
    .filter(isObject)
    .map((schedule) => {
      const name =
        typeof schedule.name === "string" && schedule.name.length > 0
          ? schedule.name
          : typeof schedule.id === "string"
            ? schedule.id
            : "schedule";
      const activeUntil =
        typeof schedule.active_until === "string"
          ? ` until ${formatDateWithDistance(schedule.active_until)}`
          : "";
      return `${name}${activeUntil}`;
    })
    .join("; ");
}

function technicalReason(): string {
  const parts = [reason];
  if (controlledReason) {
    parts.push(controlledReason);
  }
  if (blockedBy) {
    parts.push(`blocked_by=${blockedBy}`);
  }
  return parts.join(" / ");
}

function addDetail(label: string, value: string | null): void {
  if (!value) {
    return;
  }
  const details = document.getElementById("details");
  if (!details) {
    return;
  }
  const dt = document.createElement("dt");
  const dd = document.createElement("dd");
  dt.textContent = label;
  dd.textContent = value;
  if (label === "Technical reason") {
    dd.className = "muted";
  }
  details.append(dt, dd);
}

function tierTitle(value: string): string {
  if (value === "tier_1") {
    return "Tier 1";
  }
  if (value === "tier_2") {
    return "Tier 2";
  }
  return humanize(value);
}

function humanize(value: string): string {
  return value.replace(/_/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function formatDateWithDistance(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  const formatted = new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
    day: "2-digit",
    month: "short",
  }).format(date);
  const distance = formatDistance(date.getTime() - Date.now());
  return distance ? `${formatted} (${distance})` : formatted;
}

function formatDistance(milliseconds: number): string {
  if (milliseconds <= 0) {
    return "now";
  }
  const totalMinutes = Math.ceil(milliseconds / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0 && minutes > 0) {
    return `in ${hours}h ${minutes}m`;
  }
  if (hours > 0) {
    return `in ${hours}h`;
  }
  return `in ${minutes}m`;
}

function setText(id: string, value: string): void {
  const element = document.getElementById(id);
  if (element) {
    element.textContent = value;
  }
}

function isObject(value: unknown): value is ReasonData {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
})();
