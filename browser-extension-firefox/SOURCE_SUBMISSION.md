# BlocKuntu Firefox Extension Source Submission

## Build Environment

- Node.js and npm from a current Linux distribution are sufficient.
- The dependency versions are pinned by `package-lock.json`.
- The package uses the TypeScript compiler from npm.

## Rebuild The Submitted Extension

From this directory:

```bash
npm ci
npm run package:amo
```

This generates:

```text
Archive.zip
BlocKuntu.xpi
```

Both installable archives contain only:

```text
manifest.json
blocked.html
dist/background.js
dist/blocked.js
```

The TypeScript sources are in `src/`, and the packaging scripts are in
`scripts/`.
