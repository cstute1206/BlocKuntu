import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AppRulesHarness from "./AppRulesHarness.svelte";
import SiteListsHarness from "./SiteListsHarness.svelte";
import SchedulesHarness from "./SchedulesHarness.svelte";
import OverviewView from "../src/components/views/OverviewView.svelte";
import DetoxView from "../src/components/views/DetoxView.svelte";
import { mergeAppMatchers, normalizeAppRuleDraft } from "../src/lib/ui";
import type { AppRule, ConfigSnapshot, Rule, RunningApp, Schedule } from "../src/lib/types";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const appRule = (tier: AppRule["tier"] = "hard"): AppRule => ({
  id: "work", name: "Work", tier, mode: "blocklist", enabled: true,
  allowance_id: null, schedule_ids: ["always"], matchers: [{ kind: "command_name", value: "editor" }]
});
const schedule: Schedule = { id: "always", name: "Work hours", windows: [{ weekday: "everyday", start: "00:00", end: "23:59" }] };
const siteRule: Rule = { ...appRule(), patterns: [{kind: "domain", value: "work.test", match_subdomains: false}] };
const config = (tier: AppRule["tier"] = "hard"): ConfigSnapshot => ({ rules: [{ ...siteRule, tier }], app_rules: [appRule(tier)], schedules: [schedule], allowances: [] } as unknown as ConfigSnapshot);

test("Append removes empty and whitespace drafts, preserves populated rows and deduplicates", async () => {
  const rule = appRule();
  rule.matchers.push({ kind: "command_name", value: "" }, { kind: "executable_path", value: " \t " });
  const app = { pid: 10, display_name: "Game", command_name: "game", executable_basename: "game", executable_path: "/usr/bin/game", desktop_id: null, window_titles: [] } as unknown as RunningApp;
  const screen = render(AppRulesHarness, { rule, save: vi.fn(), runningApps: [app] });
  await fireEvent.click(screen.getByRole("button", { name: "Expand" }));
  await fireEvent.click(screen.getByRole("button", { name: "Append" }));
  expect(screen.queryByDisplayValue(" \t ")).toBeNull();
  expect(screen.getByDisplayValue("editor")).toBeTruthy();
  expect(screen.getByDisplayValue("/usr/bin/game")).toBeTruthy();
  const count = screen.getAllByRole("row").length;
  await fireEvent.click(screen.getByRole("button", { name: "Append" }));
  expect(screen.getAllByRole("row")).toHaveLength(count);
  const merged = mergeAppMatchers(rule.matchers, [{ kind: "command_name", value: " " }]);
  expect(merged).toEqual([{ kind: "command_name", value: "editor" }]);
  expect(normalizeAppRuleDraft({ ...rule, matchers: merged }).matchers).toEqual(merged);
});

for (const tier of ["hard", "scheduled_block", "controlled_access"] as const) {
  test(`protected key enables ${tier} application controls except during Detox`, async () => {
    const screen = render(AppRulesHarness, {rule: appRule(tier), config: config(tier), unlocked: true, save: vi.fn()});
    expect((screen.getByRole("textbox", {name: "Name"}) as HTMLInputElement).disabled).toBe(false);
    expect((screen.getByRole("button", {name: "Delete"}) as HTMLButtonElement).disabled).toBe(false);
    await screen.rerender({rule: appRule(tier), config: config(tier), unlocked: true, detox: ["work"], save: vi.fn()});
    expect((screen.getByRole("textbox", {name: "Name"}) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("button", {name: "Delete"}) as HTMLButtonElement).disabled).toBe(true);
  });
}

for (const tier of ["hard", "scheduled_block", "controlled_access"] as const) {
  test(`protected key enables ${tier} website controls except during Detox`, async () => {
    const screen = render(SiteListsHarness, {rule: {...siteRule, tier}, config: config(tier), unlocked: true});
    expect((screen.getByRole("textbox", {name: "Name"}) as HTMLInputElement).disabled).toBe(false);
    expect((screen.getByRole("button", {name: "Delete"}) as HTMLButtonElement).disabled).toBe(false);
    await screen.rerender({rule: {...siteRule, tier}, config: config(tier), unlocked: true, detox: ["work"]});
    expect((screen.getByRole("textbox", {name: "Name"}) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("button", {name: "Delete"}) as HTMLButtonElement).disabled).toBe(true);
  });
}

for (const type of ["application", "website", "schedule"]) {
  test(`${type} Delete uses the settings-style native confirmation and Cancel makes no change`, async () => {
    const remove = vi.fn();
    const screen = type === "application"
      ? render(AppRulesHarness, { rule: appRule(), config: config(), unlocked: true, save: vi.fn(), remove })
      : type === "website"
        ? render(SiteListsHarness, { config: config(), unlocked: true, remove })
        : render(SchedulesHarness, { config: {...config(), schedules:[{id:"idle",name:"Idle schedule",windows:[]}] } as ConfigSnapshot, schedule: {id:"idle", name:"Idle schedule", windows:[]}, remove });
    const action = screen.getByRole("button", {name:"Delete"});
    expect(action.classList.contains("danger-action")).toBe(true);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    await fireEvent.click(action);
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm.mock.calls[0][0]).toContain(type === "schedule" ? "Idle schedule" : "Work");
    expect(confirm.mock.calls[0][0]).toMatch(type === "schedule" ? /detached/i : /schedule attachments/i);
    expect(confirm.mock.calls[0][0]).toContain("This cannot be undone.");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(remove).not.toHaveBeenCalled();
    confirm.mockReturnValue(true);
    await fireEvent.click(action);
    expect(remove).toHaveBeenCalledTimes(1);
  });
}

test("active Detox has no cancellation action and explains the commitment", () => {
  const now = Date.now();
  const screen = render(DetoxView, {
    config: config(), detoxSessions: [{id:"detox", name:"Focus", starts_at:new Date(now).toISOString(), ends_at:new Date(now+3600000).toISOString(), cancelled_at:null, status:"active", site_rule_ids:["work"], app_rule_ids:[]}],
    detoxStarting:false, detoxMessage:null, nowMs:now, onStartDetox:vi.fn()
  });
  expect(screen.queryByRole("button", {name:/cancel/i})).toBeNull();
  expect(screen.getByText(/Once started, Detox cannot be cancelled/)).toBeTruthy();
});

test("protected key opens schedule attachment controls while Detox keeps them locked", async () => {
  const screen = render(SchedulesHarness, {config: config("scheduled_block"), schedule, unlocked:true});
  let attachments = screen.getAllByRole("checkbox") as HTMLInputElement[];
  expect(attachments.every(input => !input.disabled)).toBe(true);
  await screen.rerender({detox:["work"]});
  attachments = screen.getAllByRole("checkbox") as HTMLInputElement[];
  expect(attachments.every(input => input.disabled)).toBe(true);
});

test("Manual Unlock advertises and submits application display names", async () => {
  const unlock = vi.fn();
  const screen = render(OverviewView, {
    status:null, health:null, config:config("controlled_access"), showFirstRunOverview:false,
    urlDecision:null, urlChecking:false, unlockResult:null, unlocking:false,
    uninstallPhrase:null, tier1EditKey:null, onDismissFirstRunOverview:vi.fn(),
    onRunUrlCheck:vi.fn(), onRunUnlock:unlock
  });
  const target = screen.getByLabelText("Target") as HTMLInputElement;
  await fireEvent.input(target, {target:{value:"Work"}});
  await fireEvent.input(screen.getByRole("textbox", {name:/reason/i}), {target:{value:"I need this application to finish an urgent work task"}});
  expect(target.value).toBe("Work");
  expect(screen.getByText(/application list name/i)).toBeTruthy();
  expect(screen.container.querySelector('datalist option[value="Work"]')).toBeTruthy();
  await fireEvent.click(screen.getByRole("button", {name:"Unlock"}));
  expect(unlock).toHaveBeenCalledTimes(1);
  await screen.rerender({unlockResult:{id:1, target:"Work", rule_id:"first", rule_ids:["first", "second"],
    minutes:2, reason:"A specific work reason", started_at:new Date().toISOString(), expires_at:new Date(Date.now()+120000).toISOString()}});
  expect(screen.getByText(/\(2 lists\)/)).toBeTruthy();
});
