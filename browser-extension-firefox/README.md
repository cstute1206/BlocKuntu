# BlocKuntu Firefox Extension

This is the production Firefox WebExtension source for BlocKuntu. The source is
TypeScript and compiles to `dist/background.js`.

## Behavior

- Sends daemon heartbeats through the Native Messaging host every 5 seconds.
- Treats the backend as unhealthy until a heartbeat acknowledgement is received.
- Blocks all top-level HTTP/HTTPS navigation while the daemon heartbeat is stale
  or unavailable.
- Evaluates navigations through the daemon `evaluate_url` JSON-RPC method.
- Records visit start, visit heartbeat, and visit end events for allowed pages.
- Redirects blocked navigations to the packaged `blocked.html` page.

The extension does not make policy decisions locally. The only local decision is
the fail-closed heartbeat guard.

## Build

```bash
npm install
npm run build
npm run check
```

From the repository root, the same verification path is:

```bash
./scripts/verify-firefox-extension.sh
```

## Load Temporarily in Firefox

1. Run `npm run build`.
2. Open `about:debugging#/runtime/this-firefox`.
3. Choose "Load Temporary Add-on".
4. Select this directory's `manifest.json`.

The native host manifest must also be installed and point to
`/usr/local/bin/blockuntu-native`.

## Create Upload Packages

```bash
npm run package:amo
```

This verifies the extension, rebuilds `dist/`, and writes both `BlocKuntu.xpi`
and `Archive.zip` in this directory. These archives contain only the installable
runtime files:

```text
manifest.json
blocked.html
dist/background.js
dist/blocked.js
```

Do not upload a ZIP made from the whole `browser-extension-firefox` directory.
That includes `node_modules`, TypeScript sources, and build metadata that are
not part of the installable add-on and cause AMO validation noise.

If AMO asks for source code for review, create the separate source archive:

```bash
npm run package:source
```

Upload `Source.zip` only in the source-code field, not as the installable
extension package.
