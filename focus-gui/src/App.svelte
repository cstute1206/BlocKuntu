<script lang="ts">
  import { onMount } from "svelte";
  import {
    Activity,
    AlertTriangle,
    BarChart3,
    CalendarDays,
    CheckCircle2,
    Clock3,
    FileJson,
    Gauge,
    LayoutDashboard,
    ListChecks,
    LockKeyhole,
    Play,
    RefreshCw,
    Save,
    Search,
    Server,
    Settings,
    Shield,
    Terminal,
    Timer,
    Unlock,
    Wrench,
    XCircle
  } from "@lucide/svelte";
  import {
    configFile,
    configSnapshot,
    daemonRpc,
    daemonStatus,
    evaluateUrl,
    recentEvents,
    requestUnlock,
    systemHealth,
    writeConfigFile
  } from "./lib/api";
  import type {
    Allowance,
    ConfigSnapshot,
    DaemonStatus,
    DecisionResult,
    HealthCheck,
    RecentEvent,
    Rule,
    Schedule,
    SystemHealth,
    UnlockResult,
    ViewId
  } from "./lib/types";

  type Icon = typeof LayoutDashboard;

  const navItems: Array<{ id: ViewId; label: string; icon: Icon }> = [
    { id: "overview", label: "Dashboard", icon: LayoutDashboard },
    { id: "blocks", label: "Blocks", icon: Shield },
    { id: "config", label: "Config", icon: FileJson },
    { id: "schedule", label: "Schedule", icon: CalendarDays },
    { id: "allowances", label: "Allowances", icon: Timer },
    { id: "statistics", label: "Statistics", icon: BarChart3 },
    { id: "admin", label: "Admin", icon: Settings }
  ];

  const weekdays = [
    { id: "mon", label: "Mon" },
    { id: "tue", label: "Tue" },
    { id: "wed", label: "Wed" },
    { id: "thu", label: "Thu" },
    { id: "fri", label: "Fri" },
    { id: "sat", label: "Sat" },
    { id: "sun", label: "Sun" }
  ] as const;

  let activeView: ViewId = $state("overview");
  let socketPath = $state("");
  let status = $state<DaemonStatus | null>(null);
  let health = $state<SystemHealth | null>(null);
  let config = $state<ConfigSnapshot | null>(null);
  let events = $state<RecentEvent[]>([]);
  let loading = $state(false);
  let lastError: string | null = $state(null);
  let lastRefresh: string | null = $state(null);

  let testUrl = $state("https://youtube.com/");
  let urlDecision = $state<DecisionResult | null>(null);
  let urlChecking = $state(false);

  let unlockTarget = $state("youtube-controlled");
  let unlockMinutes = $state(10);
  let unlockReason = $state("Task-related access");
  let unlockResult = $state<UnlockResult | null>(null);
  let unlocking = $state(false);

  let selectedRuleId = $state<string | null>(null);
  let rawMethod = $state("status");
  let rawParams = $state("{}");
  let rawResult = $state("");
  let rawRunning = $state(false);
  let configPath = $state("");
  let configToml = $state("");
  let configDirty = $state(false);
  let configSaving = $state(false);
  let configMessage: string | null = $state(null);

  let hardRules = $derived(config?.rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledRules = $derived(
    config?.rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let selectedRule = $derived(
    config?.rules.find((rule) => rule.id === selectedRuleId) ?? config?.rules[0] ?? null
  );
  let eventBuckets = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const event of events) {
      counts[event.kind] = (counts[event.kind] ?? 0) + 1;
    }
    return Object.entries(counts).map(([kind, count]) => ({ kind, count })).sort(
      (a, b) => b.count - a.count
    );
  });
  let maxEventCount = $derived(Math.max(1, ...eventBuckets.map((bucket) => bucket.count)));
  let daemonOnline = $derived(status?.status === "ok");
  let failingChecks = $derived(
    health?.checks.filter((check) => check.state === "error" || check.state === "warn") ?? []
  );

  onMount(() => {
    void refreshAll();
  });

  function socketArg(): string | undefined {
    const trimmed = socketPath.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  }

  async function refreshAll(): Promise<void> {
    loading = true;
    lastError = null;

    const [statusResult, configResult, configFileResult, eventsResult, healthResult] =
      await Promise.allSettled([
      daemonStatus(socketArg()),
      configSnapshot(socketArg()),
      configFile(socketArg()),
      recentEvents(80, socketArg()),
      systemHealth(socketArg())
    ]);

    if (statusResult.status === "fulfilled") {
      status = statusResult.value;
    } else {
      status = null;
      lastError = formatError(statusResult.reason);
    }

    if (configResult.status === "fulfilled") {
      config = configResult.value;
      if (!selectedRuleId && configResult.value.rules[0]) {
        selectedRuleId = configResult.value.rules[0].id;
      }
    }

    if (configFileResult.status === "fulfilled" && !configDirty) {
      configPath = configFileResult.value.path;
      configToml = configFileResult.value.toml;
      configMessage = null;
    }

    if (eventsResult.status === "fulfilled") {
      events = eventsResult.value.events;
    }

    if (healthResult.status === "fulfilled") {
      health = healthResult.value;
    }

    lastRefresh = new Date().toLocaleTimeString();
    loading = false;
  }

  async function runUrlCheck(): Promise<void> {
    urlChecking = true;
    urlDecision = null;
    lastError = null;
    try {
      urlDecision = await evaluateUrl(testUrl, socketArg());
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      urlChecking = false;
    }
  }

  async function runUnlock(): Promise<void> {
    unlocking = true;
    unlockResult = null;
    lastError = null;
    try {
      unlockResult = await requestUnlock(unlockTarget, unlockMinutes, unlockReason, socketArg());
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      unlocking = false;
    }
  }

  async function refreshEventsOnly(): Promise<void> {
    try {
      events = (await recentEvents(80, socketArg())).events;
    } catch {
      // Full refresh surfaces connection errors; lightweight event refresh does not interrupt actions.
    }
  }

  async function runRawRpc(): Promise<void> {
    rawRunning = true;
    rawResult = "";
    lastError = null;
    try {
      const parsed = rawParams.trim() ? JSON.parse(rawParams) : {};
      const result = await daemonRpc(rawMethod, parsed, socketArg());
      rawResult = JSON.stringify(result, null, 2);
    } catch (error) {
      lastError = formatError(error);
      rawResult = "";
    } finally {
      rawRunning = false;
    }
  }

  async function reloadConfigToml(): Promise<void> {
    lastError = null;
    configMessage = null;
    try {
      const response = await configFile(socketArg());
      configPath = response.path;
      configToml = response.toml;
      configDirty = false;
      configMessage = "Reloaded from daemon config file.";
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function saveConfigToml(): Promise<void> {
    configSaving = true;
    lastError = null;
    configMessage = null;
    try {
      const response = await writeConfigFile(configToml, socketArg());
      configPath = response.path;
      config = response.config;
      configDirty = false;
      configMessage = `Saved and reloaded ${response.path}.`;
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      configSaving = false;
    }
  }

  function formatError(error: unknown): string {
    if (error instanceof Error) {
      return error.message;
    }
    return String(error);
  }

  function ruleAllowance(rule: Rule): Allowance | null {
    return config?.allowances.find((allowance) => allowance.id === rule.allowance_id) ?? null;
  }

  function checkIcon(check: HealthCheck): Icon {
    if (check.state === "ok") return CheckCircle2;
    if (check.state === "error") return XCircle;
    if (check.state === "warn") return AlertTriangle;
    return Activity;
  }

  function eventPercent(count: number): number {
    return Math.max(4, Math.round((count / maxEventCount) * 100));
  }

  function windowsFor(schedule: Schedule, weekday: string): string {
    const windows = schedule.windows.filter((window) => window.weekday === weekday);
    return windows.map((window) => `${window.start}-${window.end}`).join(", ");
  }
</script>

<div class="app-shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark"><LockKeyhole size={22} /></div>
      <div>
        <strong>BlocKuntu</strong>
        <span>Focus control</span>
      </div>
    </div>

    <nav class="nav-list" aria-label="Main">
      {#each navItems as item (item.id)}
        {@const IconComponent = item.icon}
        <button
          class:active={activeView === item.id}
          title={item.label}
          onclick={() => (activeView = item.id)}
        >
          <IconComponent size={18} aria-hidden="true" />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    <div class="daemon-strip" class:offline={!daemonOnline}>
      {#if daemonOnline}
        <CheckCircle2 size={18} aria-hidden="true" />
        <span>Daemon online</span>
      {:else}
        <XCircle size={18} aria-hidden="true" />
        <span>Daemon offline</span>
      {/if}
    </div>
  </aside>

  <main class="workspace">
    <header class="topbar">
      <div>
        <p class="eyebrow">Local enforcement</p>
        <h1>{navItems.find((item) => item.id === activeView)?.label}</h1>
      </div>
      <div class="topbar-actions">
        <label class="socket-field">
          <span>Socket</span>
          <input
            bind:value={socketPath}
            placeholder="Auto: /run/blockuntu/blockuntud.sock, then /tmp/blockuntu/blockuntud.sock"
            spellcheck="false"
          />
        </label>
        <button class="icon-button" title="Refresh" onclick={refreshAll} disabled={loading}>
          <span class:spin={loading}>
            <RefreshCw size={18} aria-hidden="true" />
          </span>
        </button>
      </div>
    </header>

    {#if lastError}
      <section class="alert-row" role="status">
        <AlertTriangle size={18} aria-hidden="true" />
        <span>{lastError}</span>
      </section>
    {/if}

    {#if activeView === "overview"}
      <section class="dashboard-grid">
        <article class="panel metric-panel">
          <div class="panel-title">
            <Server size={18} aria-hidden="true" />
            <h2>Daemon</h2>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.rules ?? "?"}</span>
            <span>rules</span>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.schedules ?? "?"}</span>
            <span>schedules</span>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.allowances ?? "?"}</span>
            <span>allowances</span>
          </div>
        </article>

        <article class="panel metric-panel">
          <div class="panel-title">
            <Shield size={18} aria-hidden="true" />
            <h2>Block Tiers</h2>
          </div>
          <div class="metric-line">
            <span class="metric-value danger">{hardRules.length}</span>
            <span>hard blocks</span>
          </div>
          <div class="metric-line">
            <span class="metric-value accent">{controlledRules.length}</span>
            <span>controlled</span>
          </div>
        </article>

        <article class="panel metric-panel">
          <div class="panel-title">
            <Wrench size={18} aria-hidden="true" />
            <h2>System</h2>
          </div>
          <div class="metric-line">
            <span class="metric-value">{health?.checks.length ?? "?"}</span>
            <span>checks</span>
          </div>
          <div class="metric-line">
            <span class="metric-value warn">{failingChecks.length}</span>
            <span>warnings</span>
          </div>
        </article>
      </section>

      <section class="content-grid">
        <article class="panel">
          <div class="panel-title">
            <Search size={18} aria-hidden="true" />
            <h2>URL Probe</h2>
          </div>
          <div class="input-row">
            <input bind:value={testUrl} placeholder="https://example.com/" />
            <button class="primary" onclick={runUrlCheck} disabled={urlChecking}>
              <Play size={17} aria-hidden="true" />
              <span>Check</span>
            </button>
          </div>
          {#if urlDecision}
            <div class:blocked={urlDecision.decision === "block"} class="decision-row">
              {#if urlDecision.decision === "block"}
                <XCircle size={18} aria-hidden="true" />
                <span>{urlDecision.reason?.kind ?? "blocked"}</span>
              {:else}
                <CheckCircle2 size={18} aria-hidden="true" />
                <span>allowed</span>
              {/if}
            </div>
          {/if}
        </article>

        <article class="panel">
          <div class="panel-title">
            <Unlock size={18} aria-hidden="true" />
            <h2>Manual Unlock</h2>
          </div>
          <div class="unlock-grid">
            <label>
              <span>Target</span>
              <input bind:value={unlockTarget} />
            </label>
            <label>
              <span>Minutes</span>
              <input type="number" min="1" max="240" bind:value={unlockMinutes} />
            </label>
            <label class="reason-field">
              <span>Reason</span>
              <input bind:value={unlockReason} />
            </label>
            <button class="primary" onclick={runUnlock} disabled={unlocking}>
              <Unlock size={17} aria-hidden="true" />
              <span>Unlock</span>
            </button>
          </div>
          {#if unlockResult}
            <p class="result-text">
              Active until {new Date(unlockResult.expires_at).toLocaleTimeString()} for
              {unlockResult.rule_id}
            </p>
          {/if}
        </article>
      </section>
    {:else if activeView === "blocks"}
      <section class="split-view">
        <article class="panel list-panel">
          <div class="panel-title">
            <ListChecks size={18} aria-hidden="true" />
            <h2>Rules</h2>
          </div>
          <div class="rule-list">
            {#each config?.rules ?? [] as rule (rule.id)}
              <button
                class:active={selectedRule?.id === rule.id}
                onclick={() => (selectedRuleId = rule.id)}
              >
                <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
                <span>{rule.name}</span>
                <em>{rule.tier === "hard" ? "Hard" : "Controlled"}</em>
              </button>
            {:else}
              <p class="empty-state">No rules reported by the daemon.</p>
            {/each}
          </div>
        </article>

        <article class="panel detail-panel">
          <div class="panel-title">
            <FileJson size={18} aria-hidden="true" />
            <h2>{selectedRule?.name ?? "Rule"}</h2>
          </div>
          {#if selectedRule}
            <div class="form-grid">
              <label>
                <span>Rule ID</span>
                <input value={selectedRule.id} readonly />
              </label>
              <label>
                <span>Tier</span>
                <input value={selectedRule.tier === "hard" ? "Hard block" : "Controlled access"} readonly />
              </label>
              <label>
                <span>Enabled</span>
                <input value={selectedRule.enabled ? "Yes" : "No"} readonly />
              </label>
              <label>
                <span>Allowance</span>
                <input value={ruleAllowance(selectedRule)?.name ?? selectedRule.allowance_id ?? "None"} readonly />
              </label>
            </div>

            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Pattern</th>
                    <th>Type</th>
                    <th>Subdomains</th>
                  </tr>
                </thead>
                <tbody>
                  {#each selectedRule.patterns as pattern}
                    <tr>
                      <td>{pattern.value}</td>
                      <td>{pattern.kind}</td>
                      <td>{pattern.match_subdomains ? "yes" : "no"}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <div class="button-row">
              <button class="secondary" disabled title="Daemon write API not implemented">
                <Save size={17} aria-hidden="true" />
                <span>Save</span>
              </button>
            </div>
          {/if}
        </article>
      </section>
    {:else if activeView === "config"}
      <section class="panel config-editor">
        <div class="panel-title">
          <FileJson size={18} aria-hidden="true" />
          <h2>TOML Configuration</h2>
        </div>
        <div class="config-toolbar">
          <label>
            <span>Path</span>
            <input value={configPath || "Unavailable"} readonly />
          </label>
          <div class="button-row">
            <button class="secondary" onclick={reloadConfigToml}>
              <RefreshCw size={17} aria-hidden="true" />
              <span>Reload</span>
            </button>
            <button
              class="primary"
              onclick={saveConfigToml}
              disabled={configSaving || !configDirty || !configToml.trim()}
            >
              <Save size={17} aria-hidden="true" />
              <span>Save TOML</span>
            </button>
          </div>
        </div>
        <textarea
          class="toml-editor"
          bind:value={configToml}
          spellcheck="false"
          oninput={() => {
            configDirty = true;
            configMessage = null;
          }}
        ></textarea>
        <div class="config-footer">
          <span>{configDirty ? "Unsaved changes" : "No unsaved changes"}</span>
          {#if configMessage}
            <strong>{configMessage}</strong>
          {/if}
        </div>
      </section>
    {:else if activeView === "schedule"}
      <section class="panel">
        <div class="panel-title">
          <CalendarDays size={18} aria-hidden="true" />
          <h2>Weekly Grid</h2>
        </div>
        <div class="schedule-grid">
          <div class="schedule-head">Schedule</div>
          {#each weekdays as day}
            <div class="schedule-head">{day.label}</div>
          {/each}
          {#each config?.schedules ?? [] as schedule (schedule.id)}
            <div class="schedule-name">{schedule.name ?? schedule.id}</div>
            {#each weekdays as day}
              <div class:filled={windowsFor(schedule, day.id)} class="schedule-cell">
                {windowsFor(schedule, day.id) || "—"}
              </div>
            {/each}
          {:else}
            <p class="empty-state">No schedules reported by the daemon.</p>
          {/each}
        </div>
      </section>
    {:else if activeView === "allowances"}
      <section class="content-grid">
        {#each config?.allowances ?? [] as allowance (allowance.id)}
          <article class="panel allowance-panel">
            <div class="panel-title">
              <Clock3 size={18} aria-hidden="true" />
              <h2>{allowance.name ?? allowance.id}</h2>
            </div>
            <div class="allowance-value">{allowance.daily_minutes} min</div>
            <p class="muted">Daily allowance</p>
          </article>
        {:else}
          <article class="panel">
            <p class="empty-state">No allowances reported by the daemon.</p>
          </article>
        {/each}
      </section>
    {:else if activeView === "statistics"}
      <section class="content-grid">
        <article class="panel">
          <div class="panel-title">
            <BarChart3 size={18} aria-hidden="true" />
            <h2>Event Mix</h2>
          </div>
          <div class="bar-list">
            {#each eventBuckets as bucket (bucket.kind)}
              <div class="bar-row">
                <span>{bucket.kind}</span>
                <div class="bar-track">
                  <div class="bar-fill" style={`width: ${eventPercent(bucket.count)}%`}></div>
                </div>
                <strong>{bucket.count}</strong>
              </div>
            {:else}
              <p class="empty-state">No events recorded yet.</p>
            {/each}
          </div>
        </article>

        <article class="panel">
          <div class="panel-title">
            <Activity size={18} aria-hidden="true" />
            <h2>Recent Events</h2>
          </div>
          <div class="event-list">
            {#each events.slice(0, 12) as event (event.id)}
              <div class="event-row">
                <span>{event.kind}</span>
                <strong>{event.target ?? "system"}</strong>
                <time>{new Date(event.created_at).toLocaleTimeString()}</time>
              </div>
            {:else}
              <p class="empty-state">No events recorded yet.</p>
            {/each}
          </div>
        </article>
      </section>
    {:else if activeView === "admin"}
      <section class="content-grid admin-grid">
        <article class="panel">
          <div class="panel-title">
            <Gauge size={18} aria-hidden="true" />
            <h2>Health</h2>
          </div>
          <div class="health-list">
            {#each health?.checks ?? [] as check (check.key)}
              {@const HealthIcon = checkIcon(check)}
              <div class="health-row" data-state={check.state}>
                <HealthIcon size={18} aria-hidden="true" />
                <span>{check.label}</span>
                <strong>{check.state}</strong>
                <small>{check.detail}</small>
              </div>
            {:else}
              <p class="empty-state">No health checks available.</p>
            {/each}
          </div>
        </article>

        <article class="panel">
          <div class="panel-title">
            <Terminal size={18} aria-hidden="true" />
            <h2>JSON-RPC</h2>
          </div>
          <div class="rpc-form">
            <label>
              <span>Method</span>
              <input bind:value={rawMethod} />
            </label>
            <label>
              <span>Params</span>
              <textarea bind:value={rawParams} spellcheck="false"></textarea>
            </label>
            <button class="primary" onclick={runRawRpc} disabled={rawRunning}>
              <Play size={17} aria-hidden="true" />
              <span>Run</span>
            </button>
          </div>
          {#if rawResult}
            <pre>{rawResult}</pre>
          {/if}
        </article>
      </section>
    {/if}

    <footer class="footer-line">
      <span>{lastRefresh ? `Last refresh ${lastRefresh}` : "Not refreshed"}</span>
      <span>{health?.socket_path ?? socketPath}</span>
    </footer>
  </main>
</div>

<style>
  .app-shell {
    display: grid;
    grid-template-columns: 236px minmax(0, 1fr);
    min-height: 100vh;
    background: #eef2ee;
  }

  .sidebar {
    background: #171b1d;
    color: #f4f7f5;
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    padding: 18px 14px;
  }

  .brand {
    align-items: center;
    display: flex;
    gap: 12px;
    min-height: 54px;
    padding: 0 8px 16px;
  }

  .brand-mark {
    align-items: center;
    background: #2fb67d;
    border-radius: 8px;
    color: #07110d;
    display: grid;
    height: 40px;
    justify-content: center;
    width: 40px;
  }

  .brand strong,
  .brand span {
    display: block;
  }

  .brand span {
    color: #aab6b0;
    font-size: 0.82rem;
  }

  .nav-list {
    display: grid;
    gap: 4px;
    margin-top: 8px;
  }

  .nav-list button {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 8px;
    color: #dce5df;
    display: flex;
    gap: 10px;
    min-height: 42px;
    padding: 0 10px;
    text-align: left;
  }

  .nav-list button.active,
  .nav-list button:hover {
    background: #27302d;
    color: #ffffff;
  }

  .daemon-strip {
    align-items: center;
    background: #20342c;
    border-radius: 8px;
    color: #c9f3df;
    display: flex;
    gap: 10px;
    margin-top: auto;
    min-height: 44px;
    padding: 0 10px;
  }

  .daemon-strip.offline {
    background: #3c2425;
    color: #ffd4d4;
  }

  .workspace {
    display: flex;
    flex-direction: column;
    min-width: 0;
    max-height: 100vh;
    overflow: auto;
    padding: 22px;
  }

  .topbar {
    align-items: center;
    display: flex;
    gap: 18px;
    justify-content: space-between;
    margin-bottom: 16px;
  }

  .eyebrow {
    color: #62706a;
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0;
    margin: 0 0 2px;
    text-transform: uppercase;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    font-size: 2rem;
    letter-spacing: 0;
    line-height: 1.1;
  }

  h2 {
    font-size: 1rem;
    letter-spacing: 0;
  }

  .topbar-actions {
    align-items: end;
    display: flex;
    gap: 10px;
  }

  .socket-field {
    display: grid;
    gap: 4px;
    min-width: min(44vw, 420px);
  }

  label span,
  .socket-field span {
    color: #63716b;
    font-size: 0.78rem;
    font-weight: 700;
  }

  input,
  textarea {
    background: #ffffff;
    border: 1px solid #cdd7d1;
    border-radius: 8px;
    color: #1d2522;
    min-height: 40px;
    min-width: 0;
    padding: 0 10px;
    width: 100%;
  }

  textarea {
    min-height: 124px;
    padding: 10px;
    resize: vertical;
  }

  .icon-button,
  .primary,
  .secondary {
    align-items: center;
    border: 0;
    border-radius: 8px;
    display: inline-flex;
    gap: 8px;
    justify-content: center;
    min-height: 40px;
    padding: 0 12px;
    white-space: nowrap;
  }

  .icon-button {
    background: #ffffff;
    border: 1px solid #cdd7d1;
    color: #1d2522;
    width: 42px;
  }

  .primary {
    background: #1f8f68;
    color: #ffffff;
  }

  .secondary {
    background: #dde5df;
    color: #34413b;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .spin {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .alert-row,
  .decision-row {
    align-items: center;
    background: #fff4d8;
    border: 1px solid #e5bc5c;
    border-radius: 8px;
    color: #5b4410;
    display: flex;
    gap: 10px;
    margin-bottom: 14px;
    min-height: 42px;
    padding: 0 12px;
  }

  .decision-row {
    background: #e6f6ee;
    border-color: #8bd3ad;
    color: #145c3d;
    margin: 12px 0 0;
  }

  .decision-row.blocked {
    background: #ffe5e5;
    border-color: #ee9d9d;
    color: #8b1d1d;
  }

  .dashboard-grid,
  .content-grid {
    display: grid;
    gap: 14px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    margin-bottom: 14px;
  }

  .content-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .admin-grid {
    align-items: start;
  }

  .panel {
    background: #ffffff;
    border: 1px solid #d5ddd8;
    border-radius: 8px;
    min-width: 0;
    padding: 16px;
  }

  .panel-title {
    align-items: center;
    color: #29332f;
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
  }

  .metric-panel {
    display: grid;
    gap: 8px;
  }

  .metric-line {
    align-items: baseline;
    display: flex;
    gap: 10px;
    justify-content: space-between;
  }

  .metric-line span:last-child,
  .muted {
    color: #64736c;
  }

  .metric-value {
    color: #1f8f68;
    font-size: 2rem;
    font-weight: 800;
    letter-spacing: 0;
  }

  .metric-value.danger {
    color: #c94f4f;
  }

  .metric-value.warn {
    color: #b87912;
  }

  .metric-value.accent {
    color: #386dc0;
  }

  .input-row,
  .button-row {
    display: flex;
    gap: 10px;
  }

  .unlock-grid,
  .form-grid,
  .rpc-form {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .config-toolbar {
    align-items: end;
    display: grid;
    gap: 12px;
    grid-template-columns: minmax(0, 1fr) max-content;
    margin-bottom: 12px;
  }

  .toml-editor {
    font-family: ui-monospace, "SFMono-Regular", Consolas, "Liberation Mono", monospace;
    min-height: 520px;
    resize: vertical;
    white-space: pre;
  }

  .config-footer {
    align-items: center;
    color: #66756e;
    display: flex;
    gap: 14px;
    justify-content: space-between;
    margin-top: 10px;
  }

  .config-footer strong {
    color: #145c3d;
    font-weight: 700;
  }

  .reason-field,
  .rpc-form label:nth-child(2) {
    grid-column: 1 / -1;
  }

  .result-text {
    color: #145c3d;
    margin-top: 12px;
  }

  .split-view {
    display: grid;
    gap: 14px;
    grid-template-columns: minmax(280px, 0.34fr) minmax(0, 1fr);
  }

  .rule-list {
    display: grid;
    gap: 6px;
  }

  .rule-list button {
    align-items: center;
    background: #f5f7f5;
    border: 1px solid #dfe6e1;
    border-radius: 8px;
    color: #26312c;
    display: grid;
    gap: 8px;
    grid-template-columns: 10px minmax(0, 1fr) max-content;
    min-height: 42px;
    padding: 0 10px;
    text-align: left;
  }

  .rule-list button.active {
    border-color: #1f8f68;
    box-shadow: inset 3px 0 0 #1f8f68;
  }

  .rule-list em {
    color: #65746d;
    font-size: 0.76rem;
    font-style: normal;
  }

  .tier-dot {
    background: #386dc0;
    border-radius: 999px;
    height: 10px;
    width: 10px;
  }

  .tier-dot.hard {
    background: #c94f4f;
  }

  .table-wrap {
    margin-top: 14px;
    overflow-x: auto;
  }

  table {
    border-collapse: collapse;
    min-width: 100%;
  }

  th,
  td {
    border-bottom: 1px solid #e2e8e4;
    padding: 10px 8px;
    text-align: left;
  }

  th {
    color: #62706a;
    font-size: 0.78rem;
    text-transform: uppercase;
  }

  .schedule-grid {
    display: grid;
    gap: 1px;
    grid-template-columns: 180px repeat(7, minmax(92px, 1fr));
    overflow-x: auto;
  }

  .schedule-head,
  .schedule-name,
  .schedule-cell {
    background: #f6f8f6;
    min-height: 42px;
    padding: 10px;
  }

  .schedule-head {
    color: #62706a;
    font-size: 0.78rem;
    font-weight: 800;
    text-transform: uppercase;
  }

  .schedule-name {
    font-weight: 700;
  }

  .schedule-cell {
    color: #68766f;
    font-size: 0.9rem;
  }

  .schedule-cell.filled {
    background: #e2f3ea;
    color: #145c3d;
    font-weight: 700;
  }

  .allowance-panel {
    min-height: 148px;
  }

  .allowance-value {
    font-size: 2.4rem;
    font-weight: 800;
    letter-spacing: 0;
  }

  .bar-list,
  .event-list,
  .health-list {
    display: grid;
    gap: 8px;
  }

  .bar-row,
  .event-row,
  .health-row {
    align-items: center;
    display: grid;
    gap: 10px;
    min-height: 36px;
  }

  .bar-row {
    grid-template-columns: 140px minmax(0, 1fr) 42px;
  }

  .bar-track {
    background: #e4ebe6;
    border-radius: 999px;
    height: 10px;
    overflow: hidden;
  }

  .bar-fill {
    background: #386dc0;
    height: 100%;
  }

  .event-row {
    grid-template-columns: 150px minmax(0, 1fr) max-content;
  }

  .event-row span,
  .event-row time {
    color: #68766f;
    font-size: 0.88rem;
  }

  .health-row {
    border-bottom: 1px solid #e2e8e4;
    grid-template-columns: 22px 170px 70px minmax(0, 1fr);
    padding-bottom: 8px;
  }

  .health-row[data-state="ok"] {
    --health-color: #1f8f68;
  }

  .health-row[data-state="warn"] {
    --health-color: #b87912;
  }

  .health-row[data-state="error"] {
    --health-color: #c94f4f;
  }

  .health-row :global(svg) {
    color: var(--health-color, #64736c);
  }

  .health-row small {
    color: #68766f;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  pre {
    background: #18201d;
    border-radius: 8px;
    color: #dcf4e8;
    font-size: 0.85rem;
    margin: 14px 0 0;
    max-height: 340px;
    overflow: auto;
    padding: 12px;
  }

  .empty-state {
    color: #66756e;
    padding: 12px 0;
  }

  .footer-line {
    color: #65736d;
    display: flex;
    font-size: 0.82rem;
    gap: 18px;
    justify-content: space-between;
    margin-top: auto;
    min-height: 32px;
    padding-top: 12px;
  }

  @media (max-width: 980px) {
    .app-shell {
      grid-template-columns: 1fr;
    }

    .sidebar {
      min-height: auto;
    }

    .nav-list {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .workspace {
      max-height: none;
    }

    .dashboard-grid,
    .content-grid,
    .split-view {
      grid-template-columns: 1fr;
    }

    .topbar,
    .topbar-actions {
      align-items: stretch;
      flex-direction: column;
    }

    .socket-field {
      min-width: 0;
    }
  }
</style>
