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
