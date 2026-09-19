# VM template configuration

`vm-templates.json` contains the non-secret repository configuration for the
three clean libvirt templates.

The domains were inspected again on 2026-09-17:

| Key | Libvirt domain | State | Package format | Libvirt snapshots |
| --- | --- | --- | --- | --- |
| `ubuntu` | `Ubuntu` | shut off | Debian `.deb` | none |
| `fedora` | `Fedora` | shut off | RPM | none |
| `cachyos` | `CachyOS` | shut off | Arch `.pkg.tar.*` | none |

The domains are treated as immutable bases. The runner creates independent
full copies of their disks and NVRAM through `virsh vol-download`, assigns new
UUIDs and MACs, and keeps the bases shut off. It uses the existing
`qemu:///system` connection and storage pools. It requires no sudo password.

CachyOS contains guest-side Snapper snapshots. These are distinct from libvirt
snapshots. The runner explicitly mounts the live `@` and `@home` Btrfs
subvolumes and checks their layout against `/etc/fstab`.

All three templates provide SSH as desktop user `akhi` on port 22. Clone
addresses are assigned by DHCP on libvirt's `default` network and are therefore
discovered at runtime; the template addresses in `../VM-Info.md` are not clone
configuration.

Verify the non-destructive base-domain assumptions with:

```bash
Testing/scripts/verify-vm-templates.sh
```

## Local connection settings

Do not commit passwords, private keys, tokens, or machine-specific secrets.
Machine-local overrides belong in `Testing/config/vm-templates.local.json`,
which is ignored by Git.

Copy `vm-templates.local.example.json` and supply the absolute path to the
client private key. The current workstation uses this shape:

```json
{
  "schema_version": 1,
  "ssh_profiles": {
    "default": {
      "identity_file": "/absolute/local/path/to/private-key"
    }
  }
}
```

The local override is configuration, not a place to store the private-key
contents. The key must be readable only by its owner. Guest connection checks
must use public-key authentication and must not prompt for a password.

## Running Phase 0

```bash
Testing/scripts/verify-vm-templates.sh
python3 Testing/scripts/phase0-vm.py run ubuntu fedora cachyos --run-id phase0-unique-run
```

Each run ID must be new. Guests run sequentially. Successful clones are shut
down, undefined and their copied disks removed; reports and screenshots remain
in `Testing/results/RUN_ID/`. Failed clones retain their disks for diagnosis.

Use `prepare`, `verify`, or `cleanup` instead of `run` for individual steps.
For example, `prepare ubuntu --run-id example` creates and prepares a stopped
clone. `verify ubuntu --run-id example` boots it, checks it and shuts it down;
`cleanup ubuntu --run-id example` removes its definition and copied storage.
The ownership record under `Testing/runtime/vm-phase0/RUN_ID/TEMPLATE/` must
remain available for verification and cleanup.

The runner validates clone UUIDs, MACs, disk/NVRAM paths, base file identities,
absence of shared disks or backing chains, and powered-off state before offline
changes. A host lock prevents two runner invocations from overlapping.
Base definitions and disk/NVRAM size, inode, mtime and ctime are compared before
and after the test. This is a metadata integrity check, not a full disk hash.

Ubuntu and Fedora use `virt-sysprep --operations machine-id,ssh-hostkeys` and
`virt-customize`. CachyOS uses equivalent guestfish operations on explicitly
mounted active subvolumes because the installed guestfs version cannot
automatically select the live OS among its Snapper snapshots.

`fixtures/guest-bootstrap.sh` configures only the disposable copy: fresh host
keys, a D-Bus machine-ID link, GDM/Plasma Login Manager automatic login, and passwordless sudo
for `akhi`. Automatic login exercises the actual Wayland desktop. The sudo
rule supports controlled reboot and later package tests. These copies are test
guests, not templates to distribute or use as general-purpose machines.

The SSH client key and `authorized_keys` stay unchanged. The runner compares
checksums of `authorized_keys` and `sshd_config` before and after preparation.
It reads the fresh public host key offline and pins it in the run's
`TEMPLATE/known_hosts`, using the clone UUID as `HostKeyAlias` and
`StrictHostKeyChecking=yes`. No interactive trust prompt or global known-hosts
change is needed. DHCP can reuse an old address; discovery matches the clone MAC.

## Host prerequisites

### DHCP and host VPN/firewall compatibility

The host must permit DHCP between a disposable guest and libvirt's DHCP server,
then SSH from the host to the guest. `virsh net-info default` reporting active
does not prove that this traffic is permitted. VPN kill switches/Network Lock
can filter it independently of libvirt's network state.

On DHCP timeout the runner saves `network-diagnostics.json` alongside the guest
result, including NIC mappings, network XML and leases. Check the stopped
guest's NetworkManager journal and the host's dnsmasq journal to distinguish
a guest DHCP failure from a lease-discovery failure. Detection of an Eddie
process is only a diagnostic clue, not proof that its Network Lock is active.

The runner never disables a VPN, changes firewall rules or restarts the shared
libvirt network. Any controlled VPN comparison or narrowly scoped firewall
exception requires the operator's approval. Preserve the original failure run
and use a new run ID for subsequent acceptance tests.


The runner needs `virsh`, `qemu-img`, `guestfish`, `virt-sysprep`,
`virt-customize`, `setfacl`, SSH, Python 3 and a C compiler. The current host
has these tools. QEMU must be able to traverse the repository's parent
directories; this workstation already grants `libvirt-qemu` traversal of the
home directory. The runner adds a QEMU read/write ACL only to copied storage.

The host's `/boot/vmlinuz-*` files are root-readable only. The following helper
downloads the exact installed running-kernel package and extracts it below
`Testing/runtime/guestfs-kernel/` for the guestfs appliance:

```bash
bash Testing/scripts/prepare-guestfs.sh
```

This does not install a kernel or change `/boot` permissions. The runner uses
the extracted kernel with its matching host modules and keeps guestfs caches
under `Testing/runtime/guestfs/`. Re-run the helper after a host kernel upgrade
if the new kernel is also unreadable. Package download needs network access.

The original domain-only `prepare-clone-identity.sh --apply DOMAIN` interface
has been replaced. Identity reset now requires a runner ownership record:
`prepare-clone-identity.sh --apply TEMPLATE RUN_ID`. Normally it is called
internally during `prepare`; it refuses an already prepared or booted clone.
