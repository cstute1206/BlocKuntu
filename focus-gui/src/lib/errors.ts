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

function humanizePolicyNouns(message: string): string {
  return message
    .replace(/Site lists/g, "Websites")
    .replace(/site lists/g, "websites")
    .replace(/Site list/g, "Website")
    .replace(/site list/g, "website")
    .replace(/App rules/g, "Applications")
    .replace(/app rules/g, "applications")
    .replace(/App rule/g, "Application")
    .replace(/app rule/g, "application");
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
    return `Website "${activeSiteList[1]}" is active right now, so it cannot be edited. Wait until it is inactive, then try again.`;
  }

  const activeAppRule = normalized.match(
    /^app rule '([^']+)' is currently active and cannot be edited$/
  );
  if (activeAppRule) {
    return `Application "${activeAppRule[1]}" is active right now, so it cannot be edited. Wait until it is inactive, then try again.`;
  }

  const detoxSiteList = normalized.match(
    /^site list '([^']+)' is covered by an active detox session and cannot be edited$/
  );
  if (detoxSiteList) {
    return `Website "${detoxSiteList[1]}" is covered by an active detox session. Unlock protected changes in Settings, then cancel the Detox session before editing it.`;
  }

  const detoxAppRule = normalized.match(
    /^app rule '([^']+)' is covered by an active detox session and cannot be edited$/
  );
  if (detoxAppRule) {
    return `Application "${detoxAppRule[1]}" is covered by an active detox session. Unlock protected changes in Settings, then cancel the Detox session before editing it.`;
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
    return `Website "${missingSiteList[1]}" no longer exists. Refresh the GUI and try again.`;
  }

  const missingAppRule = normalized.match(/^app rule '([^']+)' does not exist$/);
  if (missingAppRule) {
    return `Application "${missingAppRule[1]}" no longer exists. Refresh the GUI and try again.`;
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
    return `Allowance "${allowanceUsedBySiteList[1]}" is still used by website "${allowanceUsedBySiteList[2]}". Remove it from that website before deleting it.`;
  }

  const allowanceUsedByAppRule = normalized.match(
    /^allowance '([^']+)' is still used by app rule '([^']+)'$/
  );
  if (allowanceUsedByAppRule) {
    return `Allowance "${allowanceUsedByAppRule[1]}" is still used by application "${allowanceUsedByAppRule[2]}". Remove it from that application before deleting it.`;
  }

  const hardBlockedTarget = normalized.match(
    /^target is hard-blocked and cannot be unlocked: (.+)$/
  );
  if (hardBlockedTarget) {
    return `This target is covered by hard block "${hardBlockedTarget[1]}" and cannot be manually unlocked.`;
  }

  const detoxBlockedTarget = normalized.match(
    /^target is covered by active detox session (.+) until (.+): (.+)$/
  );
  if (detoxBlockedTarget) {
    return `Manual unlock is unavailable because rule "${detoxBlockedTarget[3]}" is in active Detox "${detoxBlockedTarget[1]}" until ${formatTimestamp(detoxBlockedTarget[2])}.`;
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

  const maxSession = normalized.match(/^requested unlock duration (\d+) exceeds maximum (\d+)$/);
  if (maxSession) {
    return `The requested unlock is too long. Maximum allowed duration: ${maxSession[2]} minutes.`;
  }

  if (normalized === "unlock target is empty") {
    return "Enter a website URL or application target before requesting an unlock.";
  }

  if (normalized === "unlock reason is required") {
    return "Enter a reason before requesting an unlock.";
  }

  const shortUnlockReason = normalized.match(
    /^unlock reason must contain at least (\d+) letters; found (\d+)$/
  );
  if (shortUnlockReason) {
    return `The unlock reason needs at least ${shortUnlockReason[1]} letters. It currently has ${shortUnlockReason[2]}.`;
  }

  if (normalized === "unlock reason has already been used") {
    return "This unlock reason has already been used. Enter a new, specific reason.";
  }

  if (normalized === "the global hourly unlock quota has been used; limit 1") {
    return "The single unlock available in the last hour has already been used.";
  }

  if (normalized === "Tier 1 edit key is required") {
    return "Enter the Tier 1 edit key before unlocking active Tier 1 edits.";
  }

  if (normalized === "Tier 1 edit unlock is required to cancel detox") {
    return "Unlock protected changes in Settings before cancelling Detox.";
  }

  if (normalized === "detox duration must be at least one minute") {
    return "Detox duration must be at least one minute.";
  }

  const detoxMaximum = normalized.match(/^detox duration cannot exceed (\d+) minutes$/);
  if (detoxMaximum) {
    return "Detox can run for at most 12 weeks.";
  }

  if (normalized === "detox needs at least one site list or app rule") {
    return "Select at least one website or application before starting detox.";
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
    return `Policy validation failed: ${capitalizeSentence(humanizePolicyNouns(normalized))}.`;
  }

  if (rpcMessage === "invalid params") {
    return capitalizeSentence(humanizePolicyNouns(normalized));
  }

  if (rpcMessage === "method not found") {
    return `The daemon does not support this request yet: ${humanizePolicyNouns(normalized)}.`;
  }

  if (rpcMessage === "internal error") {
    return `The daemon hit an internal error: ${humanizePolicyNouns(normalized)}.`;
  }

  return humanizeErrorMessage(humanizePolicyNouns(normalized));
}

function humanizeErrorMessage(message: string): string {
  const normalized = humanizePolicyNouns(message.trim());
  const lowerNormalized = normalized.toLowerCase();

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

  if (
    lowerNormalized === "operator actions are only available during sunday 20:00-23:59" ||
    lowerNormalized.includes("only available during sunday 20:00-23:59")
  ) {
    return "This action is only available on Sunday between 20:00 and 23:59.";
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
