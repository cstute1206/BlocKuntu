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

## Create an XPI

```bash
npm run build
npm run package:xpi
```

This writes `BlocKuntu.xpi` in this directory. The helper uses the system `zip`
command.
