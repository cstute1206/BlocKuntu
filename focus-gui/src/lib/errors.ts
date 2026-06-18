export function formatError(error: unknown): string {
  const rawMessage = errorToMessage(error).trim();
  const rpcError = parseJsonRpcError(rawMessage);

  if (rpcError) {
    return formatJsonRpcError(rpcError);
  }

  return humanizeErrorMessage(rawMessage);
}

interface JsonRpcErrorPayload {
  code?: unknown;
  message?: unknown;
  data?: unknown;
}

function errorToMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === "string") {
    return error;
  }

  if (error && typeof error === "object") {
    const record = error as Record<string, unknown>;
    if (typeof record.message === "string") {
      return record.message;
    }

    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }

  return String(error);
}

function parseJsonRpcError(message: string): JsonRpcErrorPayload | null {
  const daemonPrefix = "daemon JSON-RPC error:";
  const candidates = [message];
  const daemonPrefixIndex = message.indexOf(daemonPrefix);

  if (daemonPrefixIndex >= 0) {
    candidates.push(message.slice(daemonPrefixIndex + daemonPrefix.length).trim());
  }

  for (const candidate of candidates) {
    const parsed = parseJsonObject(candidate);
    if (!parsed) continue;

    const directError = jsonRpcErrorFromRecord(parsed);
    if (directError) return directError;

    const nestedError = parsed.error;
    if (nestedError && typeof nestedError === "object" && !Array.isArray(nestedError)) {
      const nestedPayload = jsonRpcErrorFromRecord(nestedError as Record<string, unknown>);
      if (nestedPayload) return nestedPayload;
    }
  }

  return null;
}

function parseJsonObject(value: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function jsonRpcErrorFromRecord(record: Record<string, unknown>): JsonRpcErrorPayload | null {
  if (!("message" in record) && !("data" in record) && !("code" in record)) {
    return null;
  }

  return {
    code: record.code,
    message: record.message,
    data: record.data
  };
}

function formatJsonRpcError(error: JsonRpcErrorPayload): string {
  const message = typeof error.message === "string" ? error.message : null;
  const detail = errorDataToMessage(error.data);

  if (detail) {
    return humanizeDaemonDetail(detail, message);
  }

  if (message) {
    return humanizeDaemonDetail(message, null);
  }

  return "The daemon returned an error, but did not include a readable message.";
}

function errorDataToMessage(data: unknown): string | null {
  if (typeof data === "string") return data;
  if (data == null) return null;

  try {
    return JSON.stringify(data);
  } catch {
    return String(data);
  }
}

function humanizeDaemonDetail(detail: string, rpcMessage: string | null): string {
  const normalized = stripKnownPrefixes(detail.trim(), [
    "invalid request:",
    "invalid configuration:",
    "daemon JSON-RPC error:"
  ]).trim();

  const activeSiteList = normalized.match(
    /^site list '([^']+)' is currently active and cannot be edited$/
  );
  if (activeSiteList) {
    return `Site list "${activeSiteList[1]}" is active right now, so it cannot be edited. Wait until it is inactive, then try again.`;
  }

  const activeAppRule = normalized.match(
    /^app rule '([^']+)' is currently active and cannot be edited$/
  );
  if (activeAppRule) {
    return `App rule "${activeAppRule[1]}" is active right now, so it cannot be edited. Wait until it is inactive, then try again.`;
  }

  const detoxSiteList = normalized.match(
    /^site list '([^']+)' is covered by an active detox session and cannot be edited$/
  );
  if (detoxSiteList) {
    return `Site list "${detoxSiteList[1]}" is covered by an active detox session. Cancel the detox session from Admin-unlocked Detox before editing it.`;
  }

  const detoxAppRule = normalized.match(
    /^app rule '([^']+)' is covered by an active detox session and cannot be edited$/
  );
  if (detoxAppRule) {
    return `App rule "${detoxAppRule[1]}" is covered by an active detox session. Cancel the detox session from Admin-unlocked Detox before editing it.`;
  }

  const activeSchedule = normalized.match(
    /^schedule '([^']+)' is currently active and cannot be edited$/
  );
  if (activeSchedule) {
    return `Schedule "${activeSchedule[1]}" is active right now, so it cannot be edited. Wait until it is inactive, then try again.`;
  }

  const activeAllowance = normalized.match(
    /^allowance '([^']+)' is currently used by an active rule and cannot be edited$/
  );
  if (activeAllowance) {
    return `Allowance "${activeAllowance[1]}" is used by an active rule right now, so it cannot be edited. Wait until the rule is inactive, then try again.`;
  }

  const missingSiteList = normalized.match(/^site list '([^']+)' does not exist$/);
  if (missingSiteList) {
    return `Site list "${missingSiteList[1]}" no longer exists. Refresh the GUI and try again.`;
  }

  const missingAppRule = normalized.match(/^app rule '([^']+)' does not exist$/);
  if (missingAppRule) {
    return `App rule "${missingAppRule[1]}" no longer exists. Refresh the GUI and try again.`;
  }

  const missingSchedule = normalized.match(/^schedule '([^']+)' does not exist$/);
  if (missingSchedule) {
    return `Schedule "${missingSchedule[1]}" no longer exists. Refresh the GUI and try again.`;
  }

  const missingAllowance = normalized.match(/^allowance '([^']+)' does not exist$/);
  if (missingAllowance) {
    return `Allowance "${missingAllowance[1]}" no longer exists. Refresh the GUI and try again.`;
  }

  const allowanceUsedBySiteList = normalized.match(
    /^allowance '([^']+)' is still used by site list '([^']+)'$/
  );
  if (allowanceUsedBySiteList) {
    return `Allowance "${allowanceUsedBySiteList[1]}" is still used by site list "${allowanceUsedBySiteList[2]}". Remove it from that list before deleting it.`;
  }

  const allowanceUsedByAppRule = normalized.match(
    /^allowance '([^']+)' is still used by app rule '([^']+)'$/
  );
  if (allowanceUsedByAppRule) {
    return `Allowance "${allowanceUsedByAppRule[1]}" is still used by app rule "${allowanceUsedByAppRule[2]}". Remove it from that app rule before deleting it.`;
  }

  const hardBlockedTarget = normalized.match(
    /^target is hard-blocked and cannot be unlocked: (.+)$/
  );
  if (hardBlockedTarget) {
    return `This target is covered by hard block "${hardBlockedTarget[1]}" and cannot be manually unlocked.`;
  }

  const unknownUnlockTarget = normalized.match(
    /^target does not match a configured controlled-access rule: (.+)$/
  );
  if (unknownUnlockTarget) {
    return `No active Tier 2 rule matches ${unknownUnlockTarget[1]}. Manual unlocks only work for active Tier 2 rules.`;
  }

  const activeUnlock = normalized.match(/^an unlock is already active for rule (.+) until (.+)$/);
  if (activeUnlock) {
    return `An unlock for rule "${activeUnlock[1]}" is already active until ${formatTimestamp(activeUnlock[2])}.`;
  }

  const cooldown = normalized.match(/^cooldown is active for rule (.+) until (.+)$/);
  if (cooldown) {
    return `Cooldown is active for rule "${cooldown[1]}". Try again after ${formatTimestamp(cooldown[2])}.`;
  }

  const hourlyQuota = normalized.match(/^hourly unlock quota exceeded for rule (.+): limit (\d+)$/);
  if (hourlyQuota) {
    return `The hourly unlock limit for rule "${hourlyQuota[1]}" has been reached. Limit: ${hourlyQuota[2]} per hour.`;
  }

  const maxSession = normalized.match(/^requested unlock duration (\d+) exceeds maximum (\d+)$/);
  if (maxSession) {
    return `The requested unlock is too long. Maximum allowed duration: ${maxSession[2]} minutes.`;
  }

  if (normalized === "unlock target is empty") {
    return "Enter a website URL or app rule target before requesting an unlock.";
  }

  if (normalized === "unlock reason is required") {
    return "Enter a reason before requesting an unlock.";
  }

  if (normalized === "unlock duration must be at least one minute") {
    return "Unlock duration must be at least one minute.";
  }

  if (normalized === "Tier 1 edit key is required") {
    return "Enter the Tier 1 edit key before unlocking active Tier 1 edits.";
  }

  if (normalized === "Tier 1 edit unlock is required to cancel detox") {
    return "Unlock the Tier 1 edit window in Admin before cancelling detox.";
  }

  if (normalized === "detox duration must be at least one minute") {
    return "Detox duration must be at least one minute.";
  }

  if (normalized === "detox needs at least one site list or app rule") {
    return "Select at least one site list or app rule before starting detox.";
  }

  const missingDetox = normalized.match(/^detox session '([^']+)' does not exist$/);
  if (missingDetox) {
    return `Detox session "${missingDetox[1]}" no longer exists. Refresh and try again.`;
  }

  if (normalized.startsWith("Tier 1 edit key is unavailable at ")) {
    return "The daemon cannot read the Tier 1 edit key. Check /etc/blockuntu/tier1-edit-key.txt and its permissions.";
  }

  if (isTimeValidationMessage(normalized)) {
    return "Use 24-hour HH:MM times, for example 09:00 or 17:30.";
  }

  if (normalized.startsWith("rule ") || normalized.startsWith("app rule ")) {
    return `Policy validation failed: ${capitalizeSentence(normalized)}.`;
  }

  if (rpcMessage === "invalid params") {
    return capitalizeSentence(normalized);
  }

  if (rpcMessage === "method not found") {
    return `The daemon does not support this request yet: ${normalized}.`;
  }

  if (rpcMessage === "internal error") {
    return `The daemon hit an internal error: ${normalized}.`;
  }

  return humanizeErrorMessage(normalized);
}

function humanizeErrorMessage(message: string): string {
  const normalized = message.trim();

  if (!normalized) {
    return "Something went wrong, but no error message was returned.";
  }

  if (normalized === "daemon returned an invalid response") {
    return "The daemon returned an invalid response. Restart the daemon and try again.";
  }

  if (normalized.includes("No such file or directory (os error 2)")) {
    return "BlocKuntu daemon is not reachable. Start the daemon or check that the service is running.";
  }

  if (normalized.includes("Connection refused")) {
    return "BlocKuntu daemon socket exists, but the daemon is not accepting connections. Restart the daemon and try again.";
  }

  if (normalized.includes("Permission denied")) {
    return "The GUI cannot access the daemon socket. Log out and back in after joining the blockuntu group, or check the socket permissions.";
  }

  if (normalized.toLowerCase().includes("timed out")) {
    return "The daemon did not respond in time. Try again or restart the daemon.";
  }

  if (normalized === "uninstall confirmation phrase does not match") {
    return "The uninstall phrase does not match. Use the displayed phrase exactly, or the recovery phrase from /etc/blockuntu/uninstall-recovery.txt.";
  }

  if (normalized === "GUI uninstall requires pkexec, but pkexec was not found") {
    return "GUI uninstall requires pkexec, but pkexec is not installed. Install pkexec or run sudo dpkg --purge blockuntu from a terminal.";
  }

  if (normalized === "Debian package blockuntu is not installed on this system") {
    return "The Debian package blockuntu is not installed on this system, so the GUI cannot purge it.";
  }

  if (normalized === "dpkg was not found on this system") {
    return "dpkg was not found on this system. Uninstall from the terminal with the package manager available on this machine.";
  }

  if (normalized.startsWith("uninstall command failed: ")) {
    return `Uninstall failed. ${capitalizeSentence(normalized.slice("uninstall command failed: ".length))}`;
  }

  if (normalized.startsWith("Tier 1 edit key is empty: ")) {
    return "The Tier 1 edit key file is empty. Reinstall the package or recreate /etc/blockuntu/tier1-edit-key.txt.";
  }

  const stripped = stripKnownPrefixes(normalized, [
    "daemon JSON-RPC error:",
    "invalid request:",
    "invalid configuration:"
  ]).trim();

  if (isTimeValidationMessage(stripped)) {
    return "Use 24-hour HH:MM times, for example 09:00 or 17:30.";
  }

  return capitalizeSentence(stripped || normalized);
}

function stripKnownPrefixes(value: string, prefixes: string[]): string {
  let result = value;
  let changed = true;

  while (changed) {
    changed = false;
    for (const prefix of prefixes) {
      if (result.toLowerCase().startsWith(prefix.toLowerCase())) {
        result = result.slice(prefix.length).trim();
        changed = true;
      }
    }
  }

  return result;
}

function isTimeValidationMessage(message: string): boolean {
  return (
    /^time '.+' must use HH:MM format$/.test(message) ||
    /^time '.+' must use zero-padded HH:MM format$/.test(message) ||
    /^invalid hour in time '.+'$/.test(message) ||
    /^invalid minute in time '.+'$/.test(message)
  );
}

function formatTimestamp(value: string): string {
  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) return value;
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    hour12: false,
    minute: "2-digit",
    second: "2-digit"
  });
}

function capitalizeSentence(value: string): string {
  if (!value) return value;
  return value[0].toUpperCase() + value.slice(1);
}
