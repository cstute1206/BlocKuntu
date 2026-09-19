# VM connection information

The three Virtual Machine Manager domains are immutable templates. Their
currently observed addresses are useful for manual inspection only; test
automation must not reuse these addresses for clones.

| Template | Observed template address | SSH user |
| --- | --- | --- |
| Ubuntu | `192.168.122.61/24` | `akhi` |
| Fedora | `192.168.122.105/24` | `akhi` |
| CachyOS | `192.168.122.91/24` | `akhi` |

All templates use the local client identity
`~/.ssh/ubuntu-to-local-VMs`. Its absolute machine-local path is stored in the
ignored `config/vm-templates.local.json`; private-key contents must never be
stored below `Testing/`.

For example, a manual connection to the Ubuntu template would be:

```bash
ssh -i ~/.ssh/ubuntu-to-local-VMs akhi@192.168.122.61
```

## Disposable clones

A full clone receives a new virtual NIC/MAC. DHCP may assign a fresh address
or reuse a previous one. After starting a clone, discover its address instead
of copying the template address:

```bash
Testing/scripts/discover-vm-ip.sh CLONE_DOMAIN
```

The Phase 0 runner copies disks and NVRAM, assigns a new UUID/MAC, prepares
guest identities offline, then verifies SSH, fixtures and the desktop:

```bash
python3 Testing/scripts/phase0-vm.py run ubuntu fedora cachyos --run-id phase0-unique-run
```

The `akhi` user's `authorized_keys` and `sshd_config` are preserved. The runner
sets up fresh host keys, unique machine IDs, automatic desktop login and
passwordless sudo in the disposable copies. Identity-reset and cleanup
operations validate ownership, UUIDs and independent disk/NVRAM paths.

The runner pins the fresh public host key read offline in
`Testing/results/RUN_ID/TEMPLATE/known_hosts`, using the clone UUID as a host
alias and `StrictHostKeyChecking=yes`. This verifies the first connection as
well as connections after reboot. Details: [config/README.md](config/README.md).
