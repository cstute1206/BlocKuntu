## ToDo:

läuft wohl auf eine Chrome Addons veröffentlichung raus. Auch Link sollte kaputt sein, weil er geupdated wurde.

```plaintext
https://nx57427.your-storageshare.de/s/3Lw3Kt6J7bkK9xe/download
```

Auch nochmal klarstellen, was dies hier macht und ob es überhaupt wirklich gebraucht wird...

```bash
./scripts/install-dev-native-host.sh
```

.pem File is in the Downloads Folder btw.

# BlocKuntu Chrome Extension

This is the Chrome/Chromium MV3 companion to the Firefox extension. It uses a  
service worker, Chrome Native Messaging, and the same daemon JSON-RPC methods as  
Firefox.

The manifest contains the public key from the currently hosted CRX, so the  
unpacked extension ID matches the force-installed extension ID:

```text
odedgejjcdilkoibeljkeohekonmdfea
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

This writes `BlocKuntu-Chrome.zip`, with `manifest.json` at the archive root,
ready for the Chrome Web Store upload form. The package deliberately excludes
source files, development dependencies, the private signing key, and the
self-hosted `.crx`. It also removes the development-only manifest `key`, which
the Chrome Web Store rejects during upload.

For the first Chrome Web Store upload, create the dashboard item but do not
publish it yet. Chrome assigns that item its own extension ID and public key.
Before publishing, replace the current self-hosted ID everywhere it is used
(the manifest key, daemon/default policy, GUI status, and Native Messaging
`allowed_origins`) with the dashboard values. Otherwise the store-installed
extension cannot connect to `blockuntu_native` and will fail closed.

The production Chrome policy currently points at this hosted CRX through a  
local update manifest written by `focusd`:

```text
https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download
```

Keep the private `.pem` used to build that CRX outside git. Future CRX builds  
must use the same private key or Chrome will assign a different extension ID.

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
