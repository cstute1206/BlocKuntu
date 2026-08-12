# BlocKuntu release checklist

BlocKuntu currently distributes standalone package files. It does not provide
update discovery or an APT/DNF repository: users download a newer package from
the GitHub Release page and install it with their normal package manager.

Publish packages as **GitHub Release assets**, never as committed files in this
repository. Build output belongs under the ignored `target/` directory; the Git
tag remains the immutable source revision from which the release was built.

## Versioning

Use one application version for the tag and release, such as `v0.1.0`, and
monotonically increasing distribution versions:

| Artifact | Example |
| --- | --- |
| Git tag and GitHub Release | `v0.1.0` |
| Debian package version | `0.1.0-1` |
| RPM version and release | `0.1.0-1` |
| Source archive | `blockuntu-0.1.0-source.tar.gz` |

When republishing the same upstream version to fix a packaging-only issue,
increase the Debian/RPM revision (for example, from `-1` to `-2`) rather than
replacing an already released package file. A later package version is required
for `apt` or `dnf` to treat it as an upgrade.

## Build and validate

1. Merge the intended changes into `main`, make sure CI is green, and create a
   signed tag from that exact commit.
2. Build the Debian package from that checkout:

   ```bash
   ./scripts/package-deb.sh --version 0.1.0-1
   ```

3. Inspect and test the resulting `target/debian/blockuntu_0.1.0-1_*.deb` in a
   clean Debian or Ubuntu VM. This is the acceptance test: verify installation,
   systemd services, GUI, blocking, browser integration, recovery/uninstall,
   and upgrade behavior.
4. Build the RPM on its supported build environment:

   ```bash
   ./scripts/package-rpm.sh --version 0.1.0 --release 1
   ```

   Test the resulting `target/rpm/*.rpm` in a clean Fedora Workstation VM with
   SELinux enforcing. Do not publish it as a stable supported artifact until
   that acceptance is complete.
5. Optionally build a source archive for source users and downstream packagers:

   ```bash
   ./scripts/package-arch.sh --version 0.1.0 --release 1 --source-only
   ```

   The output in `target/arch/` is source code, not a portable installer.
   GitHub also creates a source ZIP and tarball automatically for every tag.

## Checksums and signatures

After final acceptance, stage only the artifacts selected for publication in a
release directory. Omit the RPM or source archive lines when they are not being
published:

```bash
mkdir -p target/release
install -Dm644 target/debian/blockuntu_0.1.0-1_amd64.deb target/release/
install -Dm644 target/rpm/blockuntu-0.1.0-1.x86_64.rpm target/release/
install -Dm644 target/arch/blockuntu-0.1.0-1-source.tar.gz target/release/
(
  cd target/release
  sha256sum blockuntu_0.1.0-1_amd64.deb \
    blockuntu-0.1.0-1.x86_64.rpm \
    blockuntu-0.1.0-1-source.tar.gz > SHA256SUMS
  gpg --detach-sign --armor SHA256SUMS
)
```

Upload both `SHA256SUMS` and its detached signature `SHA256SUMS.asc`. A checksum
detects damaged downloads; a signature lets users verify that the checksums
were published by the BlocKuntu release key. Publish the public key fingerprint
and verification command in the Release notes or installation documentation.

## Publish a GitHub Release

On GitHub, create a **draft release** for the tag, upload the validated assets,
review their names and checksums, write the release notes, then publish. Keep
at least these assets together:

```text
blockuntu_0.1.0-1_amd64.deb
blockuntu-0.1.0-1.x86_64.rpm              (only after Fedora acceptance)
blockuntu-0.1.0-1-source.tar.gz           (optional)
SHA256SUMS
SHA256SUMS.asc
```

The same can be done with GitHub CLI after the release is ready:

```bash
gh release create v0.1.0 \
  --title "BlocKuntu 0.1.0" \
  --generate-notes \
  target/debian/blockuntu_0.1.0-1_amd64.deb \
  target/rpm/blockuntu-0.1.0-1.x86_64.rpm \
  target/arch/blockuntu-0.1.0-1-source.tar.gz \
  target/release/SHA256SUMS \
  target/release/SHA256SUMS.asc
```

Adjust the paths and omit assets that were not built and accepted. Do not
silently replace files on a published release; create a new package revision or
a corrected release instead.

## User installation and update commands

Release notes should state the package’s supported architecture and provide
these commands. Users should download the package and checksum files from the
same GitHub Release.

```bash
# Debian or Ubuntu
sha256sum -c SHA256SUMS
sudo apt install ./blockuntu_0.1.0-1_amd64.deb

# Fedora
sha256sum -c SHA256SUMS
sudo dnf install ./blockuntu-0.1.0-1.x86_64.rpm
```

Installing a newer local `.deb` with `apt` or a newer local RPM with `dnf`
performs the upgrade. Because there is no repository yet, users must return to
the GitHub Release page to discover and download later versions.
