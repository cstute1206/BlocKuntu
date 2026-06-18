import type {
  Allowance,
  AppMatcher,
  AppRule,
  ConfigSnapshot,
  Rule,
  RulePattern,
  Schedule,
  ScheduleDay,
  ScheduleWindow
} from "./types";

export { formatError } from "./errors";

const firstRunOverviewKey = "blockuntu.firstRunOverviewDismissed";

export const weekdays = [
  { id: "mon", label: "Mon" },
  { id: "tue", label: "Tue" },
  { id: "wed", label: "Wed" },
  { id: "thu", label: "Thu" },
  { id: "fri", label: "Fri" },
  { id: "sat", label: "Sat" },
  { id: "sun", label: "Sun" }
] as const;

export const scheduleDayChoices: Array<{ id: ScheduleDay; label: string }> = [
  { id: "everyday", label: "Everyday" },
  { id: "workdays", label: "Workdays" },
  { id: "weekend", label: "Weekend days" },
  ...weekdays
];

export const patternKinds: Array<{ id: RulePattern["kind"]; label: string }> = [
  { id: "domain", label: "Domain" },
  { id: "exact_url", label: "Exact URL" },
  { id: "url_prefix", label: "URL prefix" },
  { id: "path_prefix", label: "Path prefix" }
];

export const appMatcherKinds: Array<{ id: AppMatcher["kind"]; label: string }> = [
  { id: "command_name", label: "Command" },
  { id: "executable_basename", label: "Binary" },
  { id: "executable_path", label: "Path" },
  { id: "desktop_id", label: "Desktop ID" },
  { id: "window_title_contains", label: "Title contains" },
  { id: "window_title_exact", label: "Title exact" }
];

export const defaultDailyAllowanceMinutes = 30;

interface AllowanceOwner {
  id: string;
  name: string;
  tier: "hard" | "controlled_access";
  allowance_id?: string | null;
}

export function cloneRule(rule: Rule): Rule {
  return {
    ...rule,
    patterns: rule.patterns.map((pattern) => ({ ...pattern })),
    schedule_ids: [...rule.schedule_ids],
    allowance_id: rule.allowance_id ?? null,
    unlock_policy: rule.unlock_policy ? { ...rule.unlock_policy } : null
  };
}

export function cloneAllowance(allowance: Allowance): Allowance {
  return {
    ...allowance,
    name: allowance.name ?? ""
  };
}

export function defaultAllowanceForRule(rule: AllowanceOwner): Allowance {
  return {
    id: linkedAllowanceIdForRule(rule),
    name: allowanceNameForRule(rule),
    daily_minutes: defaultDailyAllowanceMinutes
  };
}

export function cloneAllowanceForRule(
  rule: AllowanceOwner,
  snapshot: ConfigSnapshot | null
): Allowance | null {
  if (rule.tier !== "controlled_access") return null;

  const linkedId = linkedAllowanceIdForRule(rule);
  const allowance =
    snapshot?.allowances.find((candidate) => candidate.id === rule.allowance_id) ??
    snapshot?.allowances.find((candidate) => candidate.id === linkedId);

  return allowance
    ? {
        ...cloneAllowance(allowance),
        id: linkedId,
        name: allowanceNameForRule(rule)
      }
    : defaultAllowanceForRule(rule);
}

export function cloneAppRule(rule: AppRule): AppRule {
  return {
    ...rule,
    matchers: rule.matchers.map((matcher) => ({ ...matcher })),
    schedule_ids: [...rule.schedule_ids],
    allowance_id: rule.allowance_id ?? null,
    unlock_policy: rule.unlock_policy ? { ...rule.unlock_policy } : null
  };
}

export function cloneSchedule(schedule: Schedule): Schedule {
  return {
    ...schedule,
    name: schedule.name ?? "",
    windows: schedule.windows.map((window) => ({ ...window }))
  };
}

export function normalizeRuleDraft(rule: Rule): Rule {
  return {
    ...rule,
    id: rule.id.trim(),
    name: rule.name.trim(),
    allowance_id:
      rule.tier === "controlled_access" && rule.allowance_id ? rule.allowance_id.trim() : null,
    unlock_policy: null,
    patterns: rule.patterns.map((pattern) => ({
      ...pattern,
      value: pattern.value.trim(),
      match_subdomains: pattern.kind === "domain" ? pattern.match_subdomains : false
    })),
    schedule_ids: [...rule.schedule_ids]
  };
}

export function normalizeAllowanceDraft(allowance: Allowance, rule: AllowanceOwner): Allowance {
  const dailyMinutes = Number(allowance.daily_minutes);

  return {
    ...allowance,
    id: linkedAllowanceIdForRule(rule),
    name: allowanceNameForRule(rule),
    daily_minutes: Number.isFinite(dailyMinutes) ? Math.max(0, Math.round(dailyMinutes)) : 0
  };
}

export function normalizeAppRuleDraft(rule: AppRule): AppRule {
  return {
    ...rule,
    id: rule.id.trim(),
    name: rule.name.trim(),
    allowance_id:
      rule.tier === "controlled_access" && rule.allowance_id ? rule.allowance_id.trim() : null,
    unlock_policy: null,
    matchers: rule.matchers.map((matcher) => ({
      ...matcher,
      value: matcher.value.trim()
    })),
    schedule_ids: [...rule.schedule_ids]
  };
}

export function normalizeScheduleDraft(schedule: Schedule): Schedule {
  return {
    ...schedule,
    id: schedule.id.trim(),
    name: schedule.name?.trim() || null,
    windows: schedule.windows.map((window) => ({ ...window }))
  };
}

export function ruleIsActive(rule: Rule, schedules: Schedule[]): boolean {
  if (!rule.enabled) return false;
  if (rule.schedule_ids.length === 0) return true;

  return rule.schedule_ids.some((scheduleId) => {
    const schedule = schedules.find((candidate) => candidate.id === scheduleId);
    return schedule ? scheduleIsActive(schedule) : true;
  });
}

export function appRuleIsActive(rule: AppRule, schedules: Schedule[]): boolean {
  if (!rule.enabled) return false;
  if (rule.schedule_ids.length === 0) return true;

  return rule.schedule_ids.some((scheduleId) => {
    const schedule = schedules.find((candidate) => candidate.id === scheduleId);
    return schedule ? scheduleIsActive(schedule) : true;
  });
}

export function scheduleIsActive(schedule: Schedule): boolean {
  return schedule.windows.some((window) => windowIsActive(window));
}

export function windowIsActive(window: ScheduleWindow): boolean {
  const now = new Date();
  const today = weekdays[(now.getDay() + 6) % 7].id;
  const yesterday = weekdays[(now.getDay() + 5) % 7].id;
  const currentMinute = now.getHours() * 60 + now.getMinutes();
  const start = minutesAfterMidnight(window.start);
  const end = minutesAfterMidnight(window.end);

  if (start < end) {
    return dayIncludes(window.weekday, today) && currentMinute >= start && currentMinute < end;
  }

  return (
    (dayIncludes(window.weekday, today) && currentMinute >= start) ||
    (dayIncludes(window.weekday, yesterday) && currentMinute < end)
  );
}

export function windowsFor(schedule: Schedule, weekday: ScheduleDay): string {
  const windows = schedule.windows.filter((window) => dayIncludes(window.weekday, weekday));
  return windows.map((window) => `${window.start}-${window.end}`).join(", ");
}

export function nextAvailableIndexedId(
  existingIds: string[],
  prefix: string
): { id: string; index: number } {
  const existing = new Set(existingIds);
  let index = 1;
  let id = `${prefix}-${index}`;
  while (existing.has(id)) {
    index += 1;
    id = `${prefix}-${index}`;
  }
  return { id, index };
}

export function firstRunOverviewDismissed(): boolean {
  try {
    return window.localStorage.getItem(firstRunOverviewKey) === "true";
  } catch {
    return false;
  }
}

export function markFirstRunOverviewDismissed(): void {
  try {
    window.localStorage.setItem(firstRunOverviewKey, "true");
  } catch {
    // localStorage can be unavailable in restricted WebView profiles.
  }
}

function linkedAllowanceIdForRule(rule: AllowanceOwner): string {
  return `${rule.id.trim()}-daily`;
}

function allowanceNameForRule(rule: AllowanceOwner): string {
  const name = rule.name.trim() || rule.id.trim() || "Rule";
  return `${name} daily allowance`;
}

function minutesAfterMidnight(value: string): number {
  const [hours, minutes] = value.split(":").map(Number);
  return hours * 60 + minutes;
}

function dayIncludes(scheduleDay: ScheduleDay, weekday: ScheduleDay): boolean {
  switch (scheduleDay) {
    case "everyday":
      return true;
    case "workdays":
      return ["mon", "tue", "wed", "thu", "fri"].includes(weekday);
    case "weekend":
      return weekday === "sat" || weekday === "sun";
    default:
      return scheduleDay === weekday;
  }
}
