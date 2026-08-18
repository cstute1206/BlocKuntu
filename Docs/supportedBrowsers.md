# Supported browsers

BlocKuntu provides website blocking only for browser installations that can load the BlocKuntu extension, communicate through Native Messaging, and apply the required managed browser policy. A successful extension heartbeat alone does not make a browser installation supported. Without the managed policy, the extension can be removed and private-browsing protection can be changed.

## Supported browser packages

Install the [BlocKuntu Firefox extension](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/) for Firefox-based browsers or the [BlocKuntu Chrome extension](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc) for Chromium-based browsers. After the extension's first verified heartbeat, BlocKuntu applies its managed policy where the browser package supports it.

| Browser        | Supported package installation  |
| -------------- | ------------------------------- |
| Firefox        | Native package, Snap or Flatpak |
| LibreWolf      | Native package layout           |
| Waterfox       | Native package layout           |
| Google Chrome  | Native package                  |
| Chromium       | Native package or Snap          |
| Brave          | Native package                  |
| Opera          | Native package                  |
| Microsoft Edge | Native package                  |
| Vivaldi        | Native package                  |

## Unsupported or unvalidated packages

The following package variants are not supported for browser enforcement:

| Browser package                                                            | Status        |
| -------------------------------------------------------------------------- | ------------- |
| Chromium Flatpak                                                           | Not supported |
| Brave Snap                                                                 | Not supported |
| Opera Snap                                                                 | Not supported |
| Vivaldi Snap                                                               | Not supported |
| Vivaldi Flatpak                                                            | Not supported |
| Firefox-family package layouts not listed above                           | Not supported |
