# BlocKuntu Chrome Extension

This is the Chrome/Chromium MV3 companion to the Firefox extension. It uses a
service worker, Chrome Native Messaging, and the same daemon JSON-RPC methods as
Firefox.

The manifest contains a fixed development key so the unpacked extension ID is
stable:

```text
mlfcmoellaplhamddimfpahklojgligk
```

Build and verify:

```bash
npm install
npm run verify
```

Package a local ZIP:

```bash
npm run package:zip
```

Load locally:

1. Open `chrome://extensions`.
2. Enable Developer mode.
3. Choose "Load unpacked".
4. Select `browser-extension-chrome/`.

Install the development Native Messaging host with:

```bash
./scripts/install-dev-native-host.sh
```

The host manifest is written for both Google Chrome and Chromium user profiles.
The extension is fail-closed until it receives daemon heartbeat acknowledgements
through:

```text
Chrome extension -> blockuntu_native -> blockuntud -> focus-core
```
