# Deterministic test site

The server uses only Python's standard library and loads no external assets.
It binds to loopback by default and accepts all fixture hostnames through the
HTTP `Host` header.

Start it with:

```bash
python3 Testing/fixtures/test-site/server.py \
  --bind 127.0.0.1 \
  --port 18080 \
  --ready-file Testing/runtime/test-site.json
```

The run-preparation script writes the required `/etc/hosts` fragment. The
fixture hostnames use the reserved `.test` suffix:

- `web.blockuntu.test` for exact URL, prefix, contains, path, SPA, and soak
  routes;
- `allowed.blockuntu.test` for positive website-allowlist checks;
- `outside.blockuntu.test` for negative website-allowlist checks;
- `hard.blockuntu.test`, `scheduled.blockuntu.test`, and
  `metered.blockuntu.test` for tier-specific blocklist checks.

Important routes:

| Route | Purpose |
| --- | --- |
| `/healthz` | Fixture readiness |
| `/free` | Non-matching near-miss page |
| `/exact/blocked` | Exact URL positive case |
| `/exact/blocked/child` | Exact URL negative case |
| `/prefix/blocked` and descendants | URL-prefix cases |
| `/contains?marker=blockuntu-marker` | URL-contains positive case |
| `/path/blocked` and descendants | Path-prefix cases |
| `/spa` | `history.pushState` navigation to `/spa/target` |
| `/long-running` | External-resource-free heartbeat soak page |

Use the generated URL inventory rather than duplicating ports or hostnames in
test scripts.
