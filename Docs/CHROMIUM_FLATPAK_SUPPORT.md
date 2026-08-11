# Chromium Flatpak support: design notes and lessons learned

## Status

BlocKuntu does **not** currently advertise Chromium from Flathub as a fully
managed browser.  The browser extension can communicate with BlocKuntu, but
the Chromium enterprise policies which make an extension non-removable,
disable private browsing, or enforce URL blocking are not reliably deployed
inside the Flatpak sandbox.

Chromium Flatpak is therefore an unsupported browser installation. Whenever
BlocKuntu's automatic Tier 1 blocked-browser list is active, the daemon
terminates it. This is intentionally separate from Chromium Snap, which has
its own supported package path.

This note records the work that was tried, why it was withdrawn, and the
constraints a future implementation must satisfy.  It deliberately does not
turn an experimental setup into an installation instruction.

The experimental package build `0.1.0-22` that attempted this integration was
withdrawn.  Do not use it as a reference implementation.

## Separate the two integrations

The following pieces are independent and must be reported separately in the
GUI and documentation:

| Capability | What it provides | Is a Flatpak policy required? |
| --- | --- | --- |
| Native Messaging / heartbeat | Lets the extension exchange state with the host daemon. | No, but the host manifest and executable must be visible from the sandbox. |
| Extension installation | Makes the BlocKuntu extension available to Chromium. | Not necessarily. |
| Managed browser policy | Forces the extension, prevents its removal, disables Incognito, and can apply URL policies. | Yes. |

A heartbeat or successful website block is therefore not evidence that the
browser is protected: without the managed policy, the user can disable the
extension or change its private-window permission.

## Why ordinary Chromium policy handling does not work

For a Debian or Snap Chromium installation, BlocKuntu can write the browser's
policy JSON to a host path that Chromium reads.  A Flatpak application runs in
an immutable sandbox and reads `/app/chromium/...`, not arbitrary host paths.
The policy directory visible to the browser is assembled by Flatpak when the
application starts.

The current Flathub Chromium manifest declares extension points for both
generic Chromium extensions and Chromium policies.  It merges their managed
and recommended policy directories into the browser's view.  The upstream
example of a policy extension is a **separate Flatpak runtime**, for example
`org.chromium.Chromium.Extension.<name>`, built for branch `1`, whose payload
contains `policies/managed/*.json`.

In short, the model is:

```text
system Chromium Flatpak app
        |
        +-- compatible, deployed Flatpak extension runtime
                |
                +-- policies/managed/*.json mounted read-only below /app
```

Writing a JSON file next to the installed app is not part of that model.

Upstream references:

- [Chromium's Flathub manifest](https://github.com/flathub/org.chromium.Chromium/blob/master/org.chromium.Chromium.yaml)
- [Flathub's sample Chromium policy extension](https://github.com/flathub/org.chromium.Chromium/blob/master/examples/policies/google-safe-search/org.chromium.Chromium.Extension.google-safe-search-policy.yaml)
- [Flatpak extension documentation](https://docs.flatpak.org/en/latest/extension.html)

## Failed approaches and their causes

| Attempt | Why it failed | Lesson |
| --- | --- | --- |
| Write `blockuntu.json` below `/var/lib/flatpak/extension/.../policies/managed` | A host directory is not a deployed Flatpak extension, so Flatpak did not mount it into Chromium. | Policy files must be delivered through Flatpak's extension deployment mechanism. |
| Add a host directory with `flatpak override --filesystem` | A filesystem grant exposes a host path; it cannot overlay or replace the immutable `/app/chromium/policies` directory. | Filesystem overrides can help Native Messaging, not browser policy injection. |
| Deploy an extension in user scope for a system-installed Chromium app | The extension is not in the app's matching installation scope. | The Chromium app and its policy extension must be deployed in compatible scopes. |
| Create an unsigned local remote with `--no-gpg-verify` and the Flathub collection ID | Flatpak rejects this combination: a collection-enabled remote requires GPG verification. | A distributable policy extension needs a real repository, signing key, trust setup, and collection-compatible metadata. |
| Run the experimental Flatpak setup during daemon startup | A failed `flatpak remote-add` caused the daemon to fail repeatedly, which also broke the GUI connection and made recovery harder. | Optional confined-browser support must be non-fatal, isolated from daemon startup, and expose a clear error state. |

## Why Firefox Flatpak support is not a template

Firefox's Flatpak integration uses Firefox-specific system-configuration
support (`org.mozilla.firefox.systemconfig`).  Chromium publishes different
extension points and a different directory layout.  The fact that a host path
works for Firefox does not mean that the same path, policy format, or mounting
mechanism works for Chromium.

## Why dynamic policies are especially difficult

BlocKuntu policies are dynamic: schedules, Detox, tiers, and URL rules can
change while the machine is running.  A Flatpak extension runtime is an
immutable deployment, mounted read-only into the application.  Consequently,
even a successfully installed static policy extension does not by itself solve
dynamic policy updates.

A production implementation would need a safe answer to all of these:

- How does BlocKuntu create, sign, export, and deploy an updated extension
  runtime for each policy change?
- How is the repository key installed and trusted without weakening Flatpak's
  collection and signature checks?
- When does Chromium reload a changed extension deployment, and is a browser
  restart required?
- How are failed updates rolled back while leaving existing browser protection
  and the BlocKuntu daemon healthy?
- Which scope is supported: system installations, per-user installations, or
  both?

Until those answers are implemented and tested, dynamic URL policy blocking in
Chromium Flatpak must remain unsupported.  In particular, do not promise that
Tier 3 URLs, schedules, Detox, or Incognito URL rules are enforced by policy
in this browser.

## Recommended staged path

### 1. Keep the daemon safe

Do not run Flatpak remote creation, installation, or update commands on the
daemon's critical startup path.  Treat every confined-browser deployment
failure as a recoverable, visible browser-specific status.  The daemon socket,
ordinary browser enforcement, package removal, and GUI must continue working.

### 2. Offer extension-only integration first, if useful

Native Messaging and the extension may be supported as a limited integration
provided the Flatpak bridge is explicitly configured and tested.  The GUI must
label it as **not policy-enforced** and explain that the user may disable the
extension or alter its Incognito access.  This is not equivalent to managed
browser support.

### 3. Prototype one static policy in a clean VM

Before adding product code, build a minimal, signed, collection-compatible
Flatpak extension runtime at the same scope as the installed Chromium app.
Use a static harmless policy first.  Confirm inside Chromium that the mounted
file is visible and that `chrome://policy` lists and applies it.  Then test a
static BlocKuntu force-install/private-browsing policy.

At this stage, decide the distribution model explicitly.  Shipping a local
repository and signing key is a product and security design decision, not a
small post-install script.

### 4. Add dynamic updates only after the static prototype is robust

Implement dynamic policy deployment only if a reliable signed update and
rollback mechanism is proven.  It must include policy refresh/restart
behaviour, validation before activation, cleanup on uninstall, and detailed
diagnostics.  If this cannot be made reliable, retain the static policy option
or extension-only mode rather than writing directly into Flatpak deployment
directories.

## Validation checklist

All validation belongs in a clean VM with the installed package—not only in a
source checkout.  Check the actual app scope first:

```bash
flatpak info --show-ref org.chromium.Chromium
flatpak info --show-extensions org.chromium.Chromium
```

For each supported scope, acceptance requires all of the following:

| Area | Required result |
| --- | --- |
| Mounting | The deployed extension runtime and its policy JSON are visible from inside the Chromium sandbox at the expected path. |
| Chromium policy | `chrome://policy` lists the expected policies without errors and shows their source as machine-managed. |
| Extension protection | The BlocKuntu extension is force-installed and cannot be disabled or removed. |
| Private browsing | The selected static policy (for example, private browsing disabled) takes effect. |
| Native Messaging | Heartbeat, reconnect, and restart behaviour work independently of policy status. |
| Failure handling | A bad Flatpak remote, signature, or update leaves the daemon, GUI, and existing browser protection operational and reports an actionable error. |
| Lifecycle | Package upgrade, daemon restart, browser restart, authorised uninstall, and recovery all work. |

Only after the static policy path passes this checklist should Chromium Flatpak
be listed as managed support in the product documentation.
