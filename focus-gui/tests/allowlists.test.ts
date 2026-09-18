import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AppRulesHarness from "./AppRulesHarness.svelte";
import { addAllDetectedAppMatchers, detectedMatchersForRunningApp, mergeAppMatchers } from "../src/lib/ui";
import type { AppRule, RunningApp } from "../src/lib/types";

afterEach(cleanup);
const draft = (): AppRule => ({ id: "work", name: "Work", tier: "hard", mode: "blocklist", enabled: true, allowance_id: null, schedule_ids: [], matchers: [{ kind: "command_name", value: "editor" }] });
function view(rule = draft()) {
  const save = vi.fn();
  return { save, ...render(AppRulesHarness, { rule, save }) };
}

test('PR-GUI-001 selecting application allowlist removes Tier 1 and preserves Tier 3', async () => {
  const screen = view();
  const mode = screen.getByRole('combobox', { name: /behavior/i });
  await fireEvent.change(mode, { target: { value: 'allowlist' } });
  expect((screen.getByRole('option', { name: 'Tier 1' }) as HTMLOptionElement).disabled).toBe(true);
  const tier = screen.getByRole('combobox', { name: /^tier/i }) as HTMLSelectElement;
  expect(tier.value).toBe('scheduled_block');
  await fireEvent.change(tier, { target: { value: 'controlled_access' } });
  expect(tier.value).toBe('controlled_access');
});

test('PR-GUI-002 allowlist warning precedes saving and can be cancelled', async () => {
  const screen = view({ ...draft(), mode: 'allowlist', tier: 'scheduled_block' });
  await fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
  expect(screen.save).not.toHaveBeenCalled();
  const dialog = screen.getByRole('dialog');
  for (const topic of [/session/i, /portal/i, /authentication/i, /audio/i, /clipboard/i, /browser/i, /IDE/i, /terminal/i]) expect(dialog.textContent).toMatch(topic);
  await fireEvent.click(screen.getByRole('button', { name: /^cancel$/i }));
  expect(screen.save).not.toHaveBeenCalled();
  expect(screen.queryByRole('dialog')).toBeNull();
  await fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
  await fireEvent.click(screen.getByRole('button', { name: /save allowlist/i }));
  expect(screen.save).toHaveBeenCalledTimes(1);
});

const process = (overrides: Partial<RunningApp> = {}): RunningApp => ({ pid: 10, display_name: 'Editor', executable_path: '/usr/bin/editor', executable_basename: 'editor', command_name: 'editor', desktop_id: 'editor.desktop', window_titles: [], ...overrides } as RunningApp);

test('PR-GUI-003 preferred identities use desktop, path, basename, then command', () => {
  const inputs = [process(), process({ desktop_id: null }), process({ desktop_id: null, executable_path: null }), process({ desktop_id: null, executable_path: null, executable_basename: null })];
  expect(inputs.map(app => detectedMatchersForRunningApp(app, 'allowlist'))).toEqual([
    [{ kind: 'desktop_id', value: 'editor.desktop' }], [{ kind: 'executable_path', value: '/usr/bin/editor' }], [{ kind: 'executable_basename', value: 'editor' }], [{ kind: 'command_name', value: 'editor' }]
  ]);
  const helper = process({ pid: 11, desktop_id: null, executable_path: '/usr/bin/helper' });
  const entries = addAllDetectedAppMatchers([{ kind: 'command_name', value: ' ' }], [inputs[0], inputs[0], helper], 'allowlist');
  expect(entries).toEqual([{ kind: 'desktop_id', value: 'editor.desktop' }, { kind: 'executable_path', value: '/usr/bin/helper' }]);
  expect(mergeAppMatchers(entries, entries)).toEqual(entries);
});

test('PR-GUI-001 website allowlist also disables Tier 1', async () => {
  const { default: Harness } = await import('./SiteListsHarness.svelte');
  const screen = render(Harness);
  await fireEvent.change(screen.getByRole('combobox', { name: /behavior/i }), { target: { value: 'allowlist' } });
  expect((screen.getByRole('option', { name: 'Tier 1' }) as HTMLOptionElement).disabled).toBe(true);
  expect((screen.getByRole('combobox', { name: /^tier/i }) as HTMLSelectElement).value).toBe('scheduled_block');
});
