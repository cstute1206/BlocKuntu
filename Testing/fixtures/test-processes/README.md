# Test process fixtures

`test_process.c` builds five harmless Linux processes with distinct executable
paths and command names:

| Executable | `/proc/.../comm` identity | Purpose |
| --- | --- | --- |
| `blockuntu-test-app-a` | `bk-test-app-a` | Allowed application fixture |
| `blockuntu-test-app-b` | `bk-test-app-b` | Outside-allowlist fixture |
| `blockuntu-test-block` | `bk-test-block` | Blocklist fixture |
| `blockuntu-test-parent` | `bk-test-parent` | Allowed parent fixture |
| `blockuntu-test-helper` | `bk-test-helper` | Differently identified child/helper |

Build them with:

```bash
Testing/scripts/build-test-processes.sh
```

Generated binaries are written under `Testing/artifacts/fixtures/bin/` and are
ignored by Git. Build them on the guest distribution, or on a compatible build
environment, before running process enforcement tests.

Example:

```bash
Testing/artifacts/fixtures/bin/blockuntu-test-app-a \
  --ready-file Testing/runtime/app-a.json \
  --lifetime 120

Testing/artifacts/fixtures/bin/blockuntu-test-parent \
  --spawn-helper Testing/artifacts/fixtures/bin/blockuntu-test-helper \
  --ready-file Testing/runtime/parent.json
```

The default lifetime is ten minutes so an interrupted test does not leave a
fixture running indefinitely. Sending `SIGTERM` stops the process cleanly. The
parent also terminates its helper during normal cleanup.
