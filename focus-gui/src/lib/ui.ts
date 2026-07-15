import type {
  Allowance,
  AppMatcher,
  AppRule,
  ConfigSnapshot,
  RunningApp,
  Rule,
  RulePattern,
  Schedule,
  ScheduleDay,
  ScheduleWindow
} from "./types";
import type { ViewId } from "./types";

export { formatError } from "./errors";

const firstRunOverviewKey = "blockuntu.firstRunOverviewDismissed";
const applicationUiPreferencesKey = "blockuntu.applicationUiPreferences";
const lastSelectedViewKey = "blockuntu.lastSelectedView";

export interface ApplicationUiPreferences {
  restoreLastSelectedPage: boolean;
  refreshIntervalSeconds: 5 | 15 | 30 | 60;
}

export const defaultApplicationUiPreferences: ApplicationUiPreferences = {
  restoreLastSelectedPage: false,
  refreshIntervalSeconds: 5
};

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
  { id: "url_contains", label: "URL contains" },
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
    allowance_id: rule.allowance_id ?? null
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
    allowance_id: rule.allowance_id ?? null
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
    enabled: true,
    allowance_id:
      rule.tier === "controlled_access" && rule.allowance_id ? rule.allowance_id.trim() : null,
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
    enabled: true,
    allowance_id:
      rule.tier === "controlled_access" && rule.allowance_id ? rule.allowance_id.trim() : null,
    matchers: rule.matchers.map(normalizeAppMatcherDraft),
    schedule_ids: [...rule.schedule_ids]
  };
}

export function detectedMatchersForRunningApp(app: RunningApp): AppMatcher[] {
  const candidates: Array<AppMatcher | null> = [
    app.command_name ? { kind: "command_name", value: app.command_name } : null,
    app.executable_basename ? { kind: "executable_basename", value: app.executable_basename } : null,
    app.desktop_id ? { kind: "desktop_id", value: app.desktop_id } : null,
    app.executable_path ? { kind: "executable_path", value: app.executable_path } : null
  ];

  return dedupeAppMatchers(candidates.filter((matcher): matcher is AppMatcher => matcher !== null));
}

export function mergeAppMatchers(existing: AppMatcher[], incoming: AppMatcher[]): AppMatcher[] {
  const merged = existing.map((matcher) => ({ ...matcher }));
  const seen = new Set(
    existing
      .map(normalizeAppMatcherDraft)
      .map((matcher) => `${matcher.kind}:${matcher.value}`)
  );

  for (const matcher of incoming.map(normalizeAppMatcherDraft)) {
    const key = `${matcher.kind}:${matcher.value}`;
    if (!seen.has(key)) {
      merged.push(matcher);
      seen.add(key);
    }
  }

  return merged;
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
  if (rule.tier === "hard") return true;
  if (rule.schedule_ids.length === 0) return false;

  return rule.schedule_ids.some((scheduleId) => {
    const schedule = schedules.find((candidate) => candidate.id === scheduleId);
    return schedule ? scheduleIsActive(schedule) : true;
  });
}

export function appRuleIsActive(rule: AppRule, schedules: Schedule[]): boolean {
  if (rule.tier === "hard") return true;
  if (rule.schedule_ids.length === 0) return false;

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

export function clearFirstRunOverviewDismissed(): void {
  try {
    window.localStorage.removeItem(firstRunOverviewKey);
  } catch {
    // localStorage can be unavailable in restricted WebView profiles.
  }
}

export function applicationUiPreferences(): ApplicationUiPreferences {
  try {
    const stored = window.localStorage.getItem(applicationUiPreferencesKey);
    if (!stored) return { ...defaultApplicationUiPreferences };

    const parsed = JSON.parse(stored) as Partial<ApplicationUiPreferences>;
    const refreshIntervalSeconds = [5, 15, 30, 60].includes(parsed.refreshIntervalSeconds ?? 0)
      ? (parsed.refreshIntervalSeconds as ApplicationUiPreferences["refreshIntervalSeconds"])
      : defaultApplicationUiPreferences.refreshIntervalSeconds;

    return {
      restoreLastSelectedPage: parsed.restoreLastSelectedPage === true,
      refreshIntervalSeconds
    };
  } catch {
    return { ...defaultApplicationUiPreferences };
  }
}

export function saveApplicationUiPreferences(preferences: ApplicationUiPreferences): void {
  try {
    window.localStorage.setItem(applicationUiPreferencesKey, JSON.stringify(preferences));
  } catch {
    // localStorage can be unavailable in restricted WebView profiles.
  }
}

export function lastSelectedView(): ViewId | null {
  try {
    const value = window.localStorage.getItem(lastSelectedViewKey);
    return isSavedViewId(value) ? value : null;
  } catch {
    return null;
  }
}

export function saveLastSelectedView(view: ViewId): void {
  try {
    window.localStorage.setItem(lastSelectedViewKey, view);
  } catch {
    // localStorage can be unavailable in restricted WebView profiles.
  }
}

function isSavedViewId(value: string | null): value is ViewId {
  return ["overview", "blocks", "apps", "detox", "schedule", "statistics", "admin"].includes(
    value ?? ""
  );
}

function linkedAllowanceIdForRule(rule: AllowanceOwner): string {
  return `${rule.id.trim()}-daily`;
}

function allowanceNameForRule(rule: AllowanceOwner): string {
  const name = rule.name.trim() || rule.id.trim() || "Rule";
  return `${name} daily allowance`;
}

function normalizeAppMatcherDraft(matcher: AppMatcher): AppMatcher {
  const value = matcher.value.trim();
  if (
    matcher.kind === "command_name" ||
    matcher.kind === "executable_basename" ||
    matcher.kind === "desktop_id"
  ) {
    return {
      ...matcher,
      value: value.toLowerCase()
    };
  }

  return {
    ...matcher,
    value
  };
}

function dedupeAppMatchers(matchers: AppMatcher[]): AppMatcher[] {
  const unique = new Map<string, AppMatcher>();
  for (const matcher of matchers.map(normalizeAppMatcherDraft)) {
    const key = `${matcher.kind}:${matcher.value}`;
    if (!unique.has(key)) {
      unique.set(key, matcher);
    }
  }
  return [...unique.values()];
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
