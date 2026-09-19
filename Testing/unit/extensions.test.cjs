// Execute the shipped background scripts with isolated browser APIs and a virtual clock.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const directory = process.cwd();
const firefox = directory.endsWith('firefox');
const scheme = firefox ? 'moz-extension:' : 'chrome-extension:';
const event = () => ({ listeners: [], addListener(fn) { this.listeners.push(fn); } });
const flush = async () => { for (let i = 0; i < 40; i++) await Promise.resolve(); };

async function harness() {
  let now = 1_800_000_000_000;
  let nextTimer = 1;
  const timers = new Map();
  const updates = [];
  const requests = [];
  let reply = method => method === 'extension_heartbeat' ? { browser_extension_mode: 'active' } : { decision: 'allow', metering_active: false };
  const port = { onMessage: event(), onDisconnect: event(), postMessage(message) {
    requests.push(message);
    const result = reply(message.method, message.params);
    if (result === undefined) return;
    Promise.resolve(result).then(value => port.onMessage.listeners.forEach(fn => fn({ id: message.id, result: value })), error => port.onMessage.listeners.forEach(fn => fn({ id: message.id, error: { message: error.message } })));
  }};
  const api = {
    runtime: { id: 'test-extension', getURL: value => `${scheme}//test/${value}`, getManifest: () => ({ version: 'test' }), getBrowserInfo: async () => ({ name: 'Firefox' }), connectNative: () => port },
    storage: { local: {
      get: (_, callback) => callback ? callback({}) : Promise.resolve({}),
      set: (_, callback) => callback ? callback() : Promise.resolve(),
    }},
    tabs: { onRemoved: event(), onActivated: event(),
      query: (_, callback) => callback ? callback([]) : Promise.resolve([]),
      update: (id, update, callback) => { updates.push({ id, ...update }); if (callback) callback(); return Promise.resolve(); },
    },
    webNavigation: { onBeforeNavigate: event(), onHistoryStateUpdated: event() },
    alarms: { onAlarm: event(), get: (_, callback) => callback ? callback({}) : Promise.resolve({}), create: () => Promise.resolve() },
  };
  class Clock extends Date { constructor(...args) { super(...(args.length ? args : [now])); } static now() { return now; } }
  const context = vm.createContext({ [firefox ? 'browser' : 'chrome']: api, URL, URLSearchParams, Date: Clock, navigator: { userAgent: 'Chrome/140' }, performance: { now: () => now }, console: { info() {}, warn() {}, error() {} },
    setTimeout(fn, delay) { const id = nextTimer++; timers.set(id, { fn, at: now + delay }); return id; },
    clearTimeout(id) { timers.delete(id); }, setInterval() { return nextTimer++; }, clearInterval() {},
  });
  vm.runInContext(readFileSync(path.join(directory, 'dist/background.js'), 'utf8'), context);
  await flush();
  return { api, updates, requests, context,
    run: code => vm.runInContext(code, context),
    respond: fn => { reply = fn; },
    advance: async ms => { now += ms; for (const [id, timer] of [...timers]) if (timer.at <= now) { timers.delete(id); timer.fn(); } await flush(); },
  };
}
const nav = { tabId: 7, frameId: 0, url: 'https://outside.test/work' };
const block = { decision: 'block', reason: { kind: 'scheduled_block', list_mode: 'allowlist', rule_id: 'work', rule_name: 'Work' } };

test('PR-WAL-006 top-level HTTP(S) navigation boundary', async () => {
  const h = await harness();
  for (const [url, frameId, expected] of [['https://outside.test', 0, true], ['http://outside.test', 0, true], ['https://outside.test', 1, false], ['file:///tmp/a', 0, false], ['about:config', 0, false], ['chrome://settings', 0, false], [`${scheme}//test/blocked.html`, 0, false], ['not a URL', 0, false]]) {
    assert.equal(h.run(`shouldHandleNavigation(${JSON.stringify({ ...nav, url, frameId })})`), expected, url);
  }
});

test('PR-EXT-001 navigation and SPA events use the same policy decision', async () => {
  for (const eventName of ['onBeforeNavigate', 'onHistoryStateUpdated']) {
    const h = await harness();
    h.respond(method => method === 'evaluate_url' ? block : {});
    h.api.webNavigation[eventName].listeners[0](nav);
    await flush();
    assert.equal(h.updates.length, 1);
    const url = new URL(h.updates[0].url);
    assert.equal(url.searchParams.get('url'), nav.url);
    assert.equal(JSON.parse(url.searchParams.get('reason_json')).list_mode, 'allowlist');
  }
});

test('PR-EXT-001 allowed navigation stays open; late old decisions are ignored', async () => {
  const h = await harness();
  let resolveOld;
  h.respond((method, params) => method === 'evaluate_url' && params.url.endsWith('/old') ? new Promise(resolve => { resolveOld = resolve; }) : { decision: 'allow' });
  h.api.webNavigation.onBeforeNavigate.listeners[0]({ ...nav, url: 'https://outside.test/old' });
  await flush();
  h.api.webNavigation.onHistoryStateUpdated.listeners[0](nav);
  await flush();
  resolveOld(block);
  await flush();
  assert.equal(h.updates.length, 0);
});

test('PR-EXT-001 heartbeat expiry fails closed and verified recovery restores health', async () => {
  const h = await harness();
  assert.equal(h.run('refreshHealthState()'), true);
  await h.advance(h.run("HEARTBEAT_TIMEOUT_MS"));
  assert.equal(h.run('refreshHealthState()'), true);
  await h.advance(1);
  assert.equal(h.run('refreshHealthState()'), false);
  h.respond(() => Promise.reject(new Error('offline')));
  h.api.webNavigation.onBeforeNavigate.listeners[0](nav);
  await flush();
  assert.equal(new URL(h.updates[0].url).searchParams.get('reason'), 'backend_unhealthy');
  h.respond(() => ({ browser_extension_mode: 'active' }));
  await h.run('sendHeartbeat()');
  assert.equal(h.run('refreshHealthState()'), true);
});

test('PR-EXT-001 existing-tab failures require count and elapsed grace; recovery resets failures', async () => {
  const h = await harness();
  h.respond(() => Promise.reject(new Error('offline')));
  await h.run(`revalidateTabOnce(7, ${JSON.stringify(nav.url)})`);
  await h.advance(5_000);
  await h.run(`revalidateTabOnce(7, ${JSON.stringify(nav.url)})`);
  assert.equal(h.updates.length, 0);
  h.respond(() => ({ decision: 'allow' }));
  await h.run(`revalidateTabOnce(7, ${JSON.stringify(nav.url)})`);
  assert.equal(h.run('revalidationFailures.size'), 0);
  h.respond(() => Promise.reject(new Error('offline')));
  for (let i = 0; i < 3; i++) {
    await h.run(`revalidateTabOnce(7, ${JSON.stringify(nav.url)})`);
    if (i < 2) await h.advance(5_000);
  }
  assert.equal(h.updates.length, 1);
  assert.equal(new URL(h.updates[0].url).searchParams.get('reason'), 'backend_unavailable');
});

test('PR-EXT-001 existing tabs obey a new block decision immediately', async () => {
  const h = await harness();
  h.respond(() => block);
  await h.run(`revalidateTabOnce(7, ${JSON.stringify(nav.url)})`);
  assert.equal(h.updates.length, 1);
});

test('PR-EXT-002 actual blocked-page renderer distinguishes allowlist and blocklist', () => {
  function render(mode) {
    const elements = new Map();
    const element = () => ({ textContent: '', appendChild() {}, append() {} });
    const params = new URLSearchParams({ url: nav.url, reason_json: JSON.stringify({ ...block.reason, list_mode: mode }) });
    vm.runInNewContext(readFileSync(path.join(directory, 'dist/blocked.js'), 'utf8'), {
      location: { search: `?${params}` }, URLSearchParams, Date,
      document: { getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); }, createElement: element },
    });
    return elements.get('summary').textContent;
  }
  assert.equal(render('allowlist'), 'This navigation is not included in the allowlist "Work".');
  assert.equal(render('blocklist'), 'This navigation matched the list "Work".');
});
