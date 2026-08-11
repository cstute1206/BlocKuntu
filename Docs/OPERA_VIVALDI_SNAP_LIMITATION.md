# Opera and Vivaldi Snap managed-policy limitation

## Status

Opera and Vivaldi installed as strict Snaps are currently supported for the
BlocKuntu Native Messaging bridge, but **not** for managed browser-policy
enforcement.

After the desktop-user setup and a browser restart, their store-installed
extensions can send a verified daemon heartbeat and enforce navigation through
the extension. The following protections cannot work in these Snap builds:

| Protection | Opera Snap | Vivaldi Snap |
| --- | --- | --- |
| Native Messaging and daemon heartbeat | Available | Available |
| Force-install and lock the extension | Unavailable | Unavailable |
| Disable private browsing by policy | Unavailable | Unavailable |
| Block URLs in private browsing by policy | Unavailable | Unavailable |

Do not use either Snap package where policy-backed extension locking or private
browsing protection is required. Use a browser installation that supports
managed policies instead.

## Why the policy file is not loaded

After its first verified heartbeat, BlocKuntu writes its normal policy JSON to
the host paths that the native browser packages use:

- Opera: `/etc/opt/opera/policies/managed/blockuntu.json`
- Vivaldi: `/etc/vivaldi/policies/managed/blockuntu.json`

Those files can be present, valid, and readable on the host while still being
invisible to a strict Snap. Snap runs the browser in a confined mount namespace.
The Opera and Vivaldi Snap packages currently do not map a writable Snap-data
directory onto their managed-policy paths. Their policy loaders therefore look
for `/etc/opt/opera/policies/managed` or `/etc/vivaldi/policies/managed` inside
the sandbox and find no file.

This is separate from Native Messaging. BlocKuntu installs the Native Messaging
manifest and bridge in each browser's Snap-visible per-user profile, which is
why heartbeat can succeed even though policy loading fails.

## Evidence and future validation

The clean-VM policy-loader evidence for the current affected state is:

```text
Policy file: ... (readable_expected_keys_present)
Policy loader: missing (.../policies/managed)
Result: the Snap browser did not find a mandatory platform policy file
```

`Policy file: readable` only reports the host file. The decisive condition is
that the browser process itself can see a mandatory policy file. After any
future publisher change, validate it in a clean VM and check `opera://policy`
or `vivaldi://policy` after a full browser restart.

## What would make this supportable

The Opera or Vivaldi Snap publisher must change its Snap package so its
managed-policy directory is exposed inside the confined browser filesystem—for
example, by mapping an appropriate writable Snap-data directory to the browser
policy path. This is Snap-package configuration owned by the publisher; it
cannot be added by BlocKuntu's external Debian package or by rewriting the host
policy JSON.

Once a publisher provides that mapping, BlocKuntu can mirror the policy into
the exposed Snap-data location. Until then, the observed browser behavior—not
the mere presence of the host policy JSON—defines the support boundary.

## Related documentation

- [Installation](INSTALLATION.md#opera-and-vivaldi-snap-policy-limitation)
- [Features and limitations](FEATURES.md#boundaries-and-limitations)
