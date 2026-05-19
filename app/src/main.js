import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

const icon = (name) => `/assets/icons/breeze/${name}.svg`;

const navItems = [
  ["dashboard", "dashboard", "Dashboard"],
  ["open-temporarily", "open", "Open Temporarily"],
  ["tiers", "tiers", "Tiers"],
  ["schedules", "schedules", "Schedules"],
  ["apps", "apps", "Apps"],
  ["logs-analytics", "logs", "Logs & Analytics"],
  ["settings", "settings", "Settings"],
  ["locks", "locks", "Locks"]
];

const placeholderText = {
  open: ["Open Temporarily", "Temporary Tier 2 windows will be added here."],
  tiers: ["Tiers", "Domain editing will be added here."],
  schedules: ["Schedules", "Recurring block policy editing will be added here."],
  apps: ["Apps", "Application blocking is still a future feature."],
  logs: ["Logs & Analytics", "Detailed log tables will be added here."],
  settings: ["Settings", "Configuration editing will be added here."],
  locks: ["Locks", "Lock modes will be added here."]
};

let dashboard = null;
let currentView = "dashboard";

document.querySelector("#app").innerHTML = `
  <div class="app-shell">
    <header class="titlebar">
      <div class="brand">
        <img src="${icon("security-high")}" alt="" />
        <div>
          <strong>Focus Control</strong>
          <span>focus-hosts</span>
        </div>
      </div>
      <div class="window-actions"><span></span><span></span><span></span></div>
    </header>

    <aside class="sidebar">
      <nav>
        ${navItems
          .map(
            ([iconName, view, label]) => `
              <button class="nav-item ${view === "dashboard" ? "active" : ""}" data-view="${view}">
                <img src="${icon(iconName)}" alt="" />
                <span>${label}</span>
              </button>
            `
          )
          .join("")}
      </nav>

      <section class="next-card">
        <div class="mini-label">
          <img src="${icon("next-schedule")}" alt="" />
          <span>Next schedule</span>
        </div>
        <strong id="nextScheduleName">None</strong>
        <span id="nextScheduleStarts">No upcoming schedule</span>
      </section>
      <footer>focus-hosts v0.1.0</footer>
    </aside>

    <main>
      <section class="top-row">
        <div>
          <h1 id="viewTitle">Dashboard</h1>
          <p id="viewSubtitle">Overview of your focus environment</p>
        </div>
        <div class="system-pill">
          <img src="${icon("security-high")}" alt="" />
          <div><strong id="healthText">System healthy</strong><span id="scheduleText">Loading</span></div>
        </div>
        <button class="quick-button" id="refreshButton">
          <img src="${icon("view-refresh")}" alt="" />
          Refresh
        </button>
      </section>

      <section class="view view-dashboard active">
        <div class="grid cards-top">
          <article class="card status-card">
            <h2>Current status</h2>
            <div class="status-body">
              <div class="shield"><img src="${icon("security-high")}" alt="" /></div>
              <div>
                <strong id="blockStatus">Loading</strong>
                <p id="blockDetail">Reading focus-hosts state</p>
                <p id="openingDetail">Please wait</p>
              </div>
            </div>
            <button class="full-button" id="rebuildButton">
              <img src="${icon("rebuild")}" alt="" />
              Rebuild hosts now
            </button>
          </article>

          <article class="card">
            <h2>Opens this hour</h2>
            <div class="big-number"><span id="opensUsed">0</span> / <span id="opensLimit">0</span></div>
            <p>Remaining opens</p>
            <div class="meter"><span id="opensMeter"></span></div>
            <p class="muted" id="resetsIn">Reset time unknown</p>
          </article>

          <article class="card allowance-card">
            <h2>Today's allowance</h2>
            <div id="allowanceList" class="stack-list"></div>
            <button class="link-button" data-view-link="logs">View all allowances</button>
          </article>

          <article class="card watchdog-card">
            <h2>Watchdog</h2>
            <div class="watchdog-body">
              <div class="shield small"><img src="${icon("security-high")}" alt="" /></div>
              <div><strong>Active & healthy</strong><p>No issues detected</p></div>
            </div>
            <button class="full-button" data-view-link="logs">View recent repairs</button>
          </article>
        </div>

        <div class="grid middle-grid">
          <article class="card opening-card">
            <h2>Current opening</h2>
            <div id="currentOpening"></div>
          </article>

          <article class="card activity-card">
            <h2>Recent activity</h2>
            <div id="activityList" class="activity-list"></div>
            <button class="link-button" data-view-link="logs">View all logs</button>
          </article>
        </div>

        <div class="grid lower-grid">
          <article class="card summary-card">
            <div class="card-heading">
              <h2>Today's summary</h2>
              <button class="link-button" data-view-link="logs">View full summary</button>
            </div>
            <div class="summary-stats">
              <div><span>Opens</span><strong id="summaryOpens">0</strong><small id="summaryMinutes">0m total</small></div>
              <div><span>Denied</span><strong id="summaryDenied">0</strong><small>Sites blocked</small></div>
              <div><span>Restores</span><strong id="summaryRestores">0</strong><small>All blocks</small></div>
              <div><span>Repairs</span><strong id="summaryRepairs">0</strong><small>By watchdog</small></div>
            </div>
            <div id="heatmap" class="heatmap"></div>
            <p class="muted">Activity by hour (local time)</p>
          </article>

          <article class="card">
            <h2>Top opened sites (week)</h2>
            <div id="topSites" class="rank-list"></div>
            <button class="link-button" data-view-link="logs">View analytics</button>
          </article>

          <article class="card">
            <h2>Common reasons (week)</h2>
            <div id="commonReasons" class="rank-list"></div>
            <button class="link-button" data-view-link="logs">View all reasons</button>
          </article>
        </div>
      </section>

      <section class="view view-placeholder">
        <article class="card placeholder-card">
          <h2 id="placeholderTitle">Coming next</h2>
          <p id="placeholderText">This first Tauri pass focuses on the dashboard and safe actions.</p>
        </article>
      </section>

      <footer class="status-strip">
        <span id="hostsPath">Hosts file: -</span>
        <span id="configPath">Config: -</span>
        <span id="logPath">Log: -</span>
        <button id="exportButton">
          <img src="${icon("export")}" alt="" />
          Export diagnostics
        </button>
      </footer>
    </main>
  </div>
`;

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => Array.from(document.querySelectorAll(selector));

function configPath() {
  return localStorage.getItem("focusHostsConfig") || null;
}

async function loadDashboard() {
  const raw = await invoke("dashboard_json", { configPath: configPath() });
  dashboard = JSON.parse(raw);
  renderDashboard();
}

async function runAction(command) {
  await invoke(command, { configPath: configPath() });
  await loadDashboard();
}

function renderDashboard() {
  const status = dashboard.status;
  const opening = dashboard.current_opening;
  $("#healthText").textContent = status.system_healthy ? "System healthy" : "Needs attention";
  $("#scheduleText").textContent = status.active_schedules.length
    ? `Active: ${status.active_schedules.join(", ")}`
    : "Watchdog active";
  $("#blockStatus").textContent = opening ? "Temporarily open" : "Fully blocked";
  $("#blockDetail").textContent = status.tier2_blocking_enabled
    ? "Tier 1 and Tier 2 are blocked"
    : "Tier 2 blocking disabled by schedule";
  $("#openingDetail").textContent = opening
    ? `${opening.site} closes in ${fmtDuration(opening.remaining_seconds)}`
    : "No temporary openings";
  $("#opensUsed").textContent = status.opens_used_this_hour;
  $("#opensLimit").textContent = status.open_limit_per_hour;
  $("#opensMeter").style.width = `${percent(status.opens_used_this_hour, status.open_limit_per_hour)}%`;
  $("#resetsIn").textContent = status.reset_seconds
    ? `Resets in ${fmtDuration(status.reset_seconds)}`
    : "No opens waiting to reset";
  $("#hostsPath").textContent = `Hosts file: ${dashboard.paths.hosts}`;
  $("#configPath").textContent = `Config: ${dashboard.paths.config}`;
  $("#logPath").textContent = `Log: ${dashboard.paths.log}`;

  renderNextSchedule();
  renderAllowances();
  renderOpening();
  renderActivity();
  renderSummary();
  renderRankList("#topSites", dashboard.top_sites_week, "m");
  renderRankList("#commonReasons", dashboard.common_reasons_week, "");
}

function renderNextSchedule() {
  const next = dashboard.next_schedule;
  $("#nextScheduleName").textContent = next ? next.name : "None";
  $("#nextScheduleStarts").textContent = next
    ? `starts in ${fmtDuration(next.seconds_until)}`
    : "No upcoming schedule";
}

function renderAllowances() {
  const list = $("#allowanceList");
  list.innerHTML = "";
  if (!dashboard.allowances.length) {
    list.innerHTML = `<p class="empty">No allowances configured</p>`;
    return;
  }
  dashboard.allowances.slice(0, 4).forEach((item, index) => {
    const fill = percent(item.used_minutes, item.daily_minutes);
    list.insertAdjacentHTML(
      "beforeend",
      `<div class="allowance-item">
        <div class="allowance-row">
          <strong>${escapeHtml(item.site)}</strong>
          <span>${item.used_minutes}m / ${item.daily_minutes}m</span>
        </div>
        <div class="bar"><span style="width:${fill}%; background:${colorForIndex(index)}"></span></div>
      </div>`
    );
  });
}

function renderOpening() {
  const root = $("#currentOpening");
  const opening = dashboard.current_opening;
  if (!opening) {
    root.innerHTML = `
      <p class="empty">No Tier 2 site is open right now.</p>
      <div class="opening-actions">
        <button class="full-button" id="refreshOpening">
          <img src="${icon("view-refresh")}" alt="" />
          Refresh status
        </button>
      </div>
    `;
    $("#refreshOpening").addEventListener("click", () => loadDashboard().catch(showError));
    return;
  }

  const total = opening.minutes ? opening.minutes * 60 : opening.remaining_seconds;
  const used = Math.max(0, total - opening.remaining_seconds);
  root.innerHTML = `
    <div class="opening-main">
      <div class="opening-title">
        <div class="site-badge">${escapeHtml(opening.site.slice(0, 2).toUpperCase())}</div>
        <div>
          <h3>${escapeHtml(opening.site)}</h3>
          <p>Opened for ${escapeHtml(opening.reason || "temporary access")}</p>
        </div>
      </div>
      <div>
        <div class="countdown">${fmtDuration(opening.remaining_seconds)}</div>
        <p class="muted">remaining</p>
      </div>
    </div>
    <div class="bar opening-progress"><span style="width:${percent(used, total)}%"></span></div>
    <p class="muted">Will be restored at ${fmtClock(opening.expires_at)}</p>
    <div class="opening-actions">
      <button class="full-button" id="closeOpening">
        <img src="${icon("close")}" alt="" />
        Close now
      </button>
      <button class="full-button" disabled>Extend...</button>
      <button class="full-button" ${opening.url ? "" : "disabled"} id="viewSession">View session</button>
    </div>
  `;
  $("#closeOpening").addEventListener("click", () => runAction("close_current").catch(showError));
  const view = $("#viewSession");
  if (view && opening.url) view.addEventListener("click", () => window.open(opening.url, "_blank"));
}

function renderActivity() {
  const list = $("#activityList");
  list.innerHTML = "";
  if (!dashboard.recent_activity.length) {
    list.innerHTML = `<p class="empty">No activity logged yet</p>`;
    return;
  }
  dashboard.recent_activity.forEach((entry) => {
    const label = entry.site || entry.url || "system";
    list.insertAdjacentHTML(
      "beforeend",
      `<div class="activity-row">
        <div class="activity-left">
          <span class="activity-icon ${entry.action}">${entry.action.slice(0, 2).toUpperCase()}</span>
          <div>
            <strong>${titleCase(entry.action)} ${escapeHtml(label)}</strong>
            <span>${fmtClock(entry.ts)}</span>
          </div>
        </div>
        <div class="activity-right">
          <strong>${entry.minutes ? `${entry.minutes} min` : ""}</strong>
          <span>${escapeHtml(entry.detail || entry.reason || "")}</span>
        </div>
      </div>`
    );
  });
}

function renderSummary() {
  const summary = dashboard.today_summary;
  $("#summaryOpens").textContent = summary.opens;
  $("#summaryMinutes").textContent = `${summary.opened_minutes}m total`;
  $("#summaryDenied").textContent = summary.denied;
  $("#summaryRestores").textContent = summary.restores;
  $("#summaryRepairs").textContent = summary.repairs;
  const heatmap = $("#heatmap");
  heatmap.innerHTML = "";
  const max = Math.max(1, ...summary.hourly_activity);
  summary.hourly_activity.forEach((count, hour) => {
    const level = count === 0 ? 0 : Math.max(1, Math.ceil((count / max) * 3));
    const cell = document.createElement("div");
    cell.className = `heat-cell level-${level}`;
    cell.title = `${hour}:00 - ${count} event(s)`;
    heatmap.appendChild(cell);
  });
}

function renderRankList(selector, rows, suffix) {
  const root = $(selector);
  root.innerHTML = "";
  if (!rows.length) {
    root.innerHTML = `<p class="empty">No data yet</p>`;
    return;
  }
  const max = Math.max(1, ...rows.map((row) => row.value));
  rows.forEach((row, index) => {
    root.insertAdjacentHTML(
      "beforeend",
      `<div class="rank-row">
        <span class="rank-number">${index + 1}</span>
        <div class="rank-label">
          <strong>${escapeHtml(row.label)}</strong>
          <div class="bar"><span style="width:${percent(row.value, max)}%; background:${colorForIndex(index)}"></span></div>
        </div>
        <span>${row.value}${suffix}</span>
      </div>`
    );
  });
}

function showView(view) {
  currentView = view;
  $$(".nav-item").forEach((button) => button.classList.toggle("active", button.dataset.view === view));
  $(".view-dashboard").classList.toggle("active", view === "dashboard");
  $(".view-placeholder").classList.toggle("active", view !== "dashboard");
  const [title, text] = placeholderText[view] || ["Dashboard", "Overview of your focus environment"];
  $("#viewTitle").textContent = title;
  $("#viewSubtitle").textContent = text;
  $("#placeholderTitle").textContent = title;
  $("#placeholderText").textContent = text;
}

function fmtDuration(seconds) {
  if (seconds == null) return "unknown";
  const safe = Math.max(0, Math.floor(seconds));
  const h = Math.floor(safe / 3600);
  const m = Math.floor((safe % 3600) / 60);
  const s = safe % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function fmtClock(value) {
  if (!value) return "-";
  return new Date(value).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function percent(used, total) {
  if (!total) return 0;
  return Math.max(0, Math.min(100, (used / total) * 100));
}

function colorForIndex(index) {
  return ["#ff6422", "#ffc72f", "#ff4058", "#a775ff", "#4fd278"][index % 5];
}

function titleCase(value) {
  return value.replace(/-/g, " ").replace(/\b\w/g, (ch) => ch.toUpperCase());
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function showError(error) {
  alert(error?.message || String(error));
}

$$(".nav-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view));
});

$$("[data-view-link]").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.viewLink));
});

$("#refreshButton").addEventListener("click", () => loadDashboard().catch(showError));
$("#rebuildButton").addEventListener("click", () => runAction("rebuild_hosts").catch(showError));
$("#exportButton").addEventListener("click", () => {
  const blob = new Blob([JSON.stringify(dashboard, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "blockuntu-diagnostics.json";
  link.click();
  URL.revokeObjectURL(url);
});

loadDashboard().catch((error) => {
  $("#blockStatus").textContent = "Unable to load";
  $("#blockDetail").textContent = error?.message || String(error);
});

setInterval(() => {
  if (currentView === "dashboard") loadDashboard().catch(() => {});
}, 10000);
