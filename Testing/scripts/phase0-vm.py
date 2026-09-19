#!/usr/bin/env python3
"""Full-copy, prepare and verify disposable Phase 0 libvirt guests.

All local data stays in Testing; libvirt retains only managed domain definitions.
No sudo is used. Storage is copied through the authorized libvirt connection.
"""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import fcntl
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time
import uuid
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
RUNTIME = ROOT / "runtime" / "vm-phase0"
URI = "qemu:///system"
ENV = {**os.environ, "LC_ALL": "C", "LIBGUESTFS_BACKEND": "direct"}
CONFIG = json.loads((ROOT / "config/vm-templates.json").read_text())
RUNNER_SHA256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def run(args, *, log=None, input=None, timeout=600, check=True):
    args = [str(a) for a in args]
    result = subprocess.run(args, input=input, capture_output=True, text=True,
                            timeout=timeout, env=ENV)
    if log:
        with Path(log).open("a") as file:
            file.write("$ " + shlex.join(args) + "\n" + result.stdout + result.stderr)
    if check and result.returncode:
        raise RuntimeError(f"{args[0]} failed ({result.returncode}): {result.stderr[-3000:]}")
    return result


def virsh(*args, **kwargs):
    return run(["virsh", "--connect", URI, *args], **kwargs)


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def config_guest(key):
    item = CONFIG["templates"][key]
    if item["guest_user"] != "akhi" or item["immutable_base"] is not True:
        raise ValueError("This bootstrap requires the configured immutable akhi templates")
    return item


def definition(domain):
    return ET.fromstring(virsh("dumpxml", domain, "--inactive").stdout)


def storage(xml):
    paths = []
    for disk in xml.findall("./devices/disk"):
        source = disk.find("source")
        if disk.get("device") == "cdrom" and source is None:
            continue
        if disk.get("device") != "disk" or disk.get("type") != "file":
            raise ValueError("Only file-backed disks and empty CD drives are supported")
        paths.append(Path(source.attrib["file"]).resolve())
    nvram = xml.find("./os/nvram")
    if nvram is not None:
        paths.append(Path(nvram.text).resolve())
    if xml.find("./devices/hostdev") is not None or xml.find("./devices/filesystem") is not None:
        raise ValueError("Host device/filesystem passthrough is not supported")
    return paths


def signature(path):
    stat = path.stat()
    return [stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns]


def base_records():
    result = {}
    for key, item in CONFIG["templates"].items():
        if virsh("domstate", item["domain"]).stdout.strip() != "shut off":
            raise ValueError(f"Base must be shut off: {item['domain']}")
        xml = definition(item["domain"])
        result[key] = {"uuid": xml.findtext("uuid"), "xml": ET.tostring(xml, encoding="unicode"),
                       "storage": {str(p): signature(p) for p in storage(xml)}}
    return result


def unchanged(record):
    if base_records() != record["bases"]:
        raise ValueError("Base VM definition or disk metadata changed during this run")


def guarded(record):
    """Resolve libvirt aliases; reject shared files, hardlinks and backing chains."""
    base = base_records()
    xml = definition(record["domain"])
    if xml.findtext("uuid") != record["uuid"] or xml.findtext("name") != record["domain"]:
        raise ValueError("Clone name/UUID no longer matches the ownership record")
    if record["uuid"] in [b["uuid"] for b in base.values()]:
        raise ValueError("Refusing immutable base UUID")
    sources = [s.attrib["file"] for s in xml.findall("./devices/disk/source") if "file" in s.attrib]
    if xml.findtext("./os/nvram"):
        sources.append(xml.findtext("./os/nvram"))
    if any(Path(p).is_symlink() for p in sources):
        raise ValueError("Clone storage must not be a symbolic link")
    paths = storage(xml)
    expected = [Path(p).resolve() for p in record["paths"]]
    if paths != expected or not paths:
        raise ValueError("Clone storage differs from the ownership record")
    directory = (RUNTIME / record["run_id"] / record["template"]).resolve()
    for path in paths:
        if path.parent != directory or path.is_symlink():
            raise ValueError(f"Storage outside this clone directory: {path}")
        for b in base.values():
            for original in b["storage"]:
                if path.samefile(original):
                    raise ValueError("Clone aliases a base storage file")
    # Check all other domains, including unrelated guests, before changing disks.
    for other in virsh("list", "--all", "--uuid").stdout.split():
        if other == record["uuid"]:
            continue
        other_xml = definition(other)
        clone_macs = {m.get("address").lower() for m in xml.findall("./devices/interface/mac")}
        other_macs = {m.get("address").lower() for m in other_xml.findall("./devices/interface/mac")}
        if clone_macs & other_macs:
            raise ValueError("Clone MAC address is shared with another domain")
        for source in other_xml.findall("./devices/disk/source"):
            other_path = source.get("file")
            if other_path and any(p.samefile(other_path) for p in paths):
                raise ValueError("Clone storage is shared with another domain")
        other_nvram = other_xml.findtext("./os/nvram")
        if other_nvram and any(p.samefile(other_nvram) for p in paths):
            raise ValueError("Clone NVRAM is shared with another domain")
    if virsh("domstate", record["uuid"]).stdout.strip() == "shut off":
        info = json.loads(run(["qemu-img", "info", "--output=json", paths[0]]).stdout)
        if info.get("backing-filename") or info.get("format") != "qcow2":
            raise ValueError("Only independent qcow2 copies are accepted")
    unchanged(record)
    return xml


def guestfs_setup():
    release = os.uname().release
    kernel = ROOT / "runtime/guestfs-kernel" / release / "extracted/boot" / f"vmlinuz-{release}"
    if not os.access(f"/boot/vmlinuz-{release}", os.R_OK) and not kernel.exists():
        raise ValueError("Run bash Testing/scripts/prepare-guestfs.sh first")
    if kernel.exists():
        ENV.update(SUPERMIN_KERNEL=str(kernel), SUPERMIN_MODULES=f"/lib/modules/{release}",
                   SUPERMIN_KERNEL_VERSION=release)
    for var, leaf in [("LIBGUESTFS_CACHEDIR", "cache"), ("LIBGUESTFS_TMPDIR", "tmp")]:
        path = ROOT / "runtime/guestfs" / leaf
        path.mkdir(parents=True, exist_ok=True)
        ENV[var] = str(path)


def mounts(template):
    if template != "cachyos":
        return ["-i"]
    # This template has Snapper snapshots which auto-inspection mistakes for OSes.
    # Match its actual fstab and mount only the live @ tree and its home subvolume.
    return ["-m", "/dev/sda2:/:subvol=@", "-m", "/dev/sda2:/home:subvol=@home"]


def fish(disk, commands, log, template):
    return run(["guestfish", "--ro", "--format=qcow2", "-a", disk, *mounts(template)],
               input=commands, log=log).stdout


def fingerprint(public_key):
    fields = public_key.split()
    return "SHA256:" + base64.b64encode(hashlib.sha256(base64.b64decode(fields[1])).digest()).decode().rstrip("=")


def prepare(key, run_id):
    item = config_guest(key)
    directory = RUNTIME / run_id / key
    evidence = ROOT / "results" / run_id / key
    directory.mkdir(parents=True, exist_ok=False)
    evidence.mkdir(parents=True, exist_ok=False)
    log = evidence / "host.log"
    bases = base_records()
    original = ET.fromstring(bases[key]["xml"])
    paths = storage(original)
    if len(original.findall('./devices/disk[@device="disk"]')) != 1:
        raise ValueError("Phase 0 currently requires exactly one guest disk")
    clone_uuid = str(uuid.uuid4())
    domain = f"{item['clone_name_prefix']}-{run_id}"
    if virsh("dominfo", domain, check=False).returncode == 0:
        raise ValueError(f"Domain already exists: {domain}")
    new_paths = [directory / "disk.qcow2"]
    if original.find("./os/nvram") is not None:
        new_paths.append(directory / "VARS.fd")
    if shutil.disk_usage(directory).free < sum(p.stat().st_size for p in paths) + 5 * 1024**3:
        raise ValueError("Insufficient space for an independent disk copy and 5 GiB reserve")
    record = {"schema_version": 1, "run_id": run_id, "template": key, "domain": domain,
              "uuid": clone_uuid, "paths": [str(p) for p in new_paths], "bases": bases,
              "stage": "copying", "evidence": str(evidence)}
    save(directory / "owner.json", record)
    print(f"{key}: copying powered-off template storage", flush=True)
    for src, dst in zip(paths, new_paths):
        # Download via libvirt instead of requiring root access to base files.
        virsh("vol-download", str(src), str(dst), "--sparse", log=log, timeout=1800)
        dst.chmod(0o600)
    unchanged(record)
    info = json.loads(run(["qemu-img", "info", "--output=json", new_paths[0]], log=log).stdout)
    if info.get("backing-filename") or info.get("format") != "qcow2":
        raise ValueError("Source must be a standalone qcow2 image")
    # Record original identities from the untouched copy, never by booting bases.
    commands = "\n".join([
        f"download /etc/machine-id {shlex.quote(str(evidence / 'base-machine-id'))}",
        f"download /etc/ssh/ssh_host_ed25519_key.pub {shlex.quote(str(evidence / 'base-ssh.pub'))}",
        "checksum sha256 /home/akhi/.ssh/authorized_keys",
        "checksum sha256 /etc/ssh/sshd_config",
        "cat /etc/os-release",
    ]) + "\n"
    record["original_guest"] = fish(new_paths[0], commands, log, key)
    if key == "cachyos":
        fstab = fish(new_paths[0], "cat /etc/fstab\n", log, key)
        if not re.search(r"\s/\s+btrfs\s+subvol=/@,", fstab) or not re.search(r"\s/home\s+btrfs\s+subvol=/@home,", fstab):
            raise ValueError("CachyOS fstab differs from the supported active subvolume layout")
    save(evidence / "base-guest.json", {"inspection": record["original_guest"]})
    original.find("name").text = domain
    original.find("uuid").text = clone_uuid
    original.find('./devices/disk[@device="disk"]/source').set("file", str(new_paths[0]))
    if len(new_paths) == 2:
        original.find("./os/nvram").text = str(new_paths[1])
    for interface in original.findall("./devices/interface"):
        if interface.get("type") != "network" or interface.find("source").get("network") != "default":
            raise ValueError("Only the configured default libvirt network is supported")
        interface.find("mac").set("address", "52:54:00:" + ":".join(f"{b:02x}" for b in os.urandom(3)))
    for graphics in original.findall("./devices/graphics"):
        graphics.set("listen", "127.0.0.1")
        for listener in graphics.findall("listen"):
            listener.set("address", "127.0.0.1")
    ET.SubElement(original, "description").text = f"BlocKuntu disposable Phase 0 clone; run={run_id}"
    xml_path = evidence / "clone.xml"
    ET.indent(original)
    ET.ElementTree(original).write(xml_path, encoding="unicode")
    # Existing host ACL permits qemu traversal of the home directory.
    for path in new_paths:
        run(["setfacl", "-m", "u:libvirt-qemu:rw", path], log=log)
    virsh("define", xml_path, log=log)
    record["stage"] = "defined"
    save(directory / "owner.json", record)
    prepare_identity(record)
    return record


def prepare_identity(record):
    if record["stage"] != "defined":
        raise ValueError("Identity preparation requires a fresh owned clone at stage defined")
    guarded(record)
    if virsh("domstate", record["uuid"]).stdout.strip() != "shut off":
        raise ValueError("Clone must be shut off")
    evidence = Path(record["evidence"])
    log = evidence / "host.log"
    disk = record["paths"][0]
    print(f"{record['template']}: preparing guest identities and desktop", flush=True)
    if record["template"] == "cachyos":
        # virt-sysprep/customize cannot select one OS among Snapper snapshots.
        # Apply the same reset to explicitly mounted live subvolumes using guestfish.
        bootstrap = shlex.quote(str(ROOT / "fixtures/guest-bootstrap.sh"))
        commands = '\n'.join([
            'write /etc/machine-id ""',
            'glob rm-f /etc/ssh/ssh_host_*',
            f'upload {bootstrap} /tmp/blockuntu-phase0-bootstrap.sh',
            'sh "/bin/sh /tmp/blockuntu-phase0-bootstrap.sh"',
            'rm /tmp/blockuntu-phase0-bootstrap.sh',
        ]) + '\n'
        run(["guestfish", "--rw", "--format=qcow2", "-a", disk, *mounts("cachyos")], input=commands, log=log)
    else:
        run(["virt-sysprep", "--format", "qcow2", "-a", disk,
             "--operations", "machine-id,ssh-hostkeys"], log=log)
        run(["virt-customize", "--format", "qcow2", "-a", disk, "--no-network",
             "--run", ROOT / "fixtures/guest-bootstrap.sh"], log=log)
    inspection = fish(disk, "\n".join([
        f"download /etc/ssh/ssh_host_ed25519_key.pub {shlex.quote(str(evidence / 'clone-ssh.pub'))}",
        "checksum sha256 /home/akhi/.ssh/authorized_keys",
        "checksum sha256 /etc/ssh/sshd_config",
    ]) + "\n", log, record["template"])
    if inspection.splitlines()[:2] != record["original_guest"].splitlines()[:2]:
        raise ValueError("authorized_keys or sshd_config changed")
    if fingerprint((evidence / "clone-ssh.pub").read_text()) == fingerprint((evidence / "base-ssh.pub").read_text()):
        raise ValueError("SSH host identity was not regenerated")
    (evidence / "known_hosts").write_text(record["uuid"] + " " + (evidence / "clone-ssh.pub").read_text())
    record["stage"] = "prepared"
    record["host_fingerprint"] = fingerprint((evidence / "clone-ssh.pub").read_text())
    save(RUNTIME / record["run_id"] / record["template"] / "owner.json", record)
    unchanged(record)


def ssh(record, command, **kwargs):
    item = config_guest(record["template"])
    local = json.loads((ROOT / "config/vm-templates.local.json").read_text())
    key = Path(local["ssh_profiles"][item["transport"]["identity_profile"]]["identity_file"])
    if not key.is_absolute() or key.stat().st_mode & 0o077:
        raise ValueError("SSH private key must have an absolute path and owner-only permissions")
    kwargs.setdefault("timeout", 30)
    return run(["ssh", "-F", "/dev/null", "-i", key,
                "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5",
                "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3",
                "-o", "StrictHostKeyChecking=yes", "-o", f"HostKeyAlias={record['uuid']}",
                "-o", f"UserKnownHostsFile={Path(record['evidence']) / 'known_hosts'}",
                "-o", "GlobalKnownHostsFile=/dev/null", "-o", "HostKeyAlgorithms=ssh-ed25519",
                f"{item['guest_user']}@{record['ip']}", "/bin/sh -c " + shlex.quote(command)], **kwargs)


def wait_until(action, description, seconds=180):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        try:
            value = action()
            if value:
                return value
        except (RuntimeError, subprocess.TimeoutExpired) as error:
            last = error
        time.sleep(2)
    raise TimeoutError(f"Timed out waiting for {description}: {last}")


def address(record):
    xml = definition(record["uuid"])
    macs = {m.get("address").lower() for m in xml.findall("./devices/interface/mac")}
    lines = virsh("domifaddr", record["uuid"], "--source", "lease").stdout.splitlines()
    addresses = {str(ipaddress.ip_interface(fields[3]).ip) for line in lines
                 if len(fields := line.split()) == 4 and fields[1].lower() in macs and fields[2] == "ipv4"}
    if len(addresses) > 1:
        raise ValueError("Ambiguous IPv4 leases")
    return next(iter(addresses), None)


def network_diagnostics(record):
    """Read-only evidence; never reset networks or weaken a host VPN/firewall."""
    commands = [["virsh", "--connect", URI, "domiflist", record["uuid"]],
                ["virsh", "--connect", URI, "domifaddr", record["uuid"], "--source", "lease"]]
    xml = definition(record["uuid"])
    networks = sorted({source.get("network") for source in
                       xml.findall("./devices/interface/source") if source.get("network")})
    for network in networks:
        for action in ("net-info", "net-dumpxml", "net-dhcp-leases"):
            commands.append(["virsh", "--connect", URI, action, network])
    commands.append(["ip", "-brief", "link", "show"])
    commands.append(["pgrep", "-x", "eddie-ui|eddie-cli"])
    evidence = []
    for command in commands:
        try:
            answer = run(command, check=False, timeout=10)
            evidence.append({"command": command, "returncode": answer.returncode,
                             "stdout": answer.stdout, "stderr": answer.stderr})
        except (OSError, subprocess.TimeoutExpired) as error:
            evidence.append({"command": command, "error": str(error)})
    save(Path(record["evidence"]) / "network-diagnostics.json", evidence)


def wait_for_address(record):
    try:
        return wait_until(lambda: address(record), "clone DHCP lease")
    except TimeoutError as error:
        try:
            network_diagnostics(record)
        except Exception as diagnostic_error:
            print(f"Network diagnostic collection failed: {diagnostic_error}", file=sys.stderr)
        raise TimeoutError(
            "Clone received no discoverable DHCP lease within 180 seconds. "
            "Check network-diagnostics.json, guest NetworkManager logs and host DHCP logs. "
            "An active libvirt network does not prove DHCP is reachable; host VPN Network Lock "
            "or firewall rules can block it. No VPN/firewall settings were changed."
        ) from error


def graphical_check(record, suffix):
    evidence = Path(record["evidence"])
    log = evidence / "host.log"
    wait_until(lambda: "WAYLAND_DISPLAY=" in ssh(record, "systemctl --user show-environment", check=False).stdout
               or "DISPLAY=" in ssh(record, "systemctl --user show-environment", check=False).stdout,
               "desktop session", seconds=240)
    session = ssh(record, "loginctl show-user akhi -p Display --value", log=log).stdout.strip()
    properties = ssh(record, f"loginctl show-session {shlex.quote(session)} -p Type -p Class -p Active -p Seat -p Name", log=log).stdout
    if "Active=yes" not in properties or "Class=user" not in properties or "Seat=seat0" not in properties:
        raise ValueError(f"No active local desktop session: {properties}")
    ssh(record, "mkdir -p Testing/phase0; rm -f Testing/phase0/gui-ready.json", log=log)
    ssh(record, "cat > Testing/phase0/gui-smoke.py", input=(ROOT / "fixtures/gui-smoke.py").read_text(), log=log)
    ssh(record, "systemd-run --user --collect --unit=blockuntu-phase0-gui --working-directory=/home/akhi/Testing/phase0 /usr/bin/python3 /home/akhi/Testing/phase0/gui-smoke.py", log=log)
    wait_until(lambda: ssh(record, "test -s Testing/phase0/gui-ready.json", check=False).returncode == 0, "mapped GTK window", seconds=30)
    gui = json.loads(ssh(record, "cat Testing/phase0/gui-ready.json", log=log).stdout)
    if gui["mapped"] is not True or gui["uid"] != int(ssh(record, "id -u").stdout):
        raise ValueError("GUI is not mapped as the expected desktop user")
    virsh("screenshot", record["uuid"], evidence / f"desktop{suffix}.png", log=log)
    ssh(record, "systemctl --user stop blockuntu-phase0-gui", log=log)
    return properties, gui


def fixtures_check(record):
    log = Path(record["evidence"]) / "host.log"
    ssh(record, "mkdir -p Testing/phase0/fixtures", log=log)
    for source in [ROOT / "fixtures/test-site/server.py", ROOT / "fixtures/guest-smoke.py"]:
        ssh(record, f"cat > Testing/phase0/fixtures/{source.name}", input=source.read_text(), log=log)
    for source in sorted((ROOT / "artifacts/fixtures/bin").glob("blockuntu-test-*")):
        ssh(record, f"base64 -d > Testing/phase0/fixtures/{source.name} && chmod 755 Testing/phase0/fixtures/{source.name}",
            input=base64.b64encode(source.read_bytes()).decode(), log=log)
    output = ssh(record, "cd Testing/phase0/fixtures && /usr/bin/python3 guest-smoke.py", log=log, timeout=90).stdout
    return json.loads(output.splitlines()[-1])


def verify(record):
    if record["stage"] != "prepared":
        raise ValueError("Verification requires a prepared clone")
    guarded(record)
    evidence = Path(record["evidence"])
    log = evidence / "host.log"
    result = {"template": record["template"], "domain": record["domain"], "status": "running", "checks": {},
              "started_at": dt.datetime.now(dt.timezone.utc).isoformat(), "clone_uuid": record["uuid"],
              "base_uuid": record["bases"][record["template"]]["uuid"],
              "runner_sha256": RUNNER_SHA256}
    try:
        virsh("start", record["uuid"], log=log)
        record["ip"] = wait_for_address(record)
        print(f"{record['template']}: {record['ip']}; waiting for SSH and desktop", flush=True)
        wait_until(lambda: ssh(record, "true", check=False, log=log).returncode == 0, "SSH")
        result["checks"]["ssh_pinned_host_key"] = True
        result["ip"] = record["ip"]
        machine_id = ssh(record, "cat /etc/machine-id", log=log).stdout.strip()
        if not re.fullmatch("[0-9a-f]{32}", machine_id) or machine_id == (evidence / "base-machine-id").read_text().strip():
            raise ValueError("Machine ID was not regenerated")
        result.update(machine_id=machine_id, host_fingerprint=record["host_fingerprint"])
        result["checks"]["unique_machine_id"] = True
        result["checks"]["unique_ssh_host_key"] = True
        result["checks"]["authorized_keys_and_sshd_config_preserved"] = True
        result["guest"] = ssh(record, "cat /etc/os-release; uname -r; id; sudo -n true; loginctl list-sessions --no-legend", log=log).stdout
        package_queries = {"ubuntu": "test \"$(dpkg-query -W -f='${Status}' blockuntu 2>/dev/null)\" = 'install ok installed'",
                           "fedora": "rpm -q blockuntu", "cachyos": "pacman -Q blockuntu"}
        baseline = "if " + package_queries[record["template"]] + "; then echo 'BlocKuntu package is already installed' >&2; exit 1; fi; "
        baseline += "if command -v blockuntud || systemctl is-active --quiet blockuntu.service; then echo 'BlocKuntu binary/service already present' >&2; exit 1; fi"
        ssh(record, baseline, log=log)
        result["checks"]["blockuntu_not_installed_or_running"] = True
        result["desktop"], result["gui"] = graphical_check(record, "")
        result["checks"]["graphical_window_mapped"] = True
        result["checks"]["graphical_window_stopped"] = True
        result["fixtures"] = fixtures_check(record)
        result["checks"]["fixtures_start_and_stop_twice"] = True
        # Verify SSH host trust and machine ID also survive a reboot.
        boot_id = ssh(record, "cat /proc/sys/kernel/random/boot_id").stdout.strip()
        ssh(record, "sudo -n systemctl reboot", check=False, log=log)
        time.sleep(5)
        def rebooted():
            record["ip"] = address(record) or record["ip"]
            answer = ssh(record, "cat /proc/sys/kernel/random/boot_id", check=False)
            return answer.returncode == 0 and answer.stdout.strip() != boot_id
        wait_until(rebooted, "SSH after reboot")
        if ssh(record, "cat /etc/machine-id").stdout.strip() != machine_id:
            raise ValueError("Machine ID changed on reboot")
        result["checks"]["identity_stable_after_reboot"] = True
        result["desktop_after_reboot"], result["gui_after_reboot"] = graphical_check(record, "-after-reboot")
        result["checks"]["graphical_window_after_reboot"] = True
        result["status"] = "passed"
    except Exception as error:
        result.update(status="failed", error=str(error))
        virsh("screenshot", record["uuid"], evidence / "failure.png", check=False, log=log)
        raise
    finally:
        # Keep failed disks and definitions for diagnosis, but release guest RAM.
        if virsh("domstate", record["uuid"]).stdout.strip() != "shut off":
            virsh("shutdown", record["uuid"], log=log)
            try:
                wait_until(lambda: virsh("domstate", record["uuid"]).stdout.strip() == "shut off", "guest shutdown", seconds=90)
            except TimeoutError:
                # Only this recorded disposable clone can reach this branch.
                guarded(record)
                virsh("destroy", record["uuid"], log=log)
        try:
            unchanged(record)
            result["checks"]["base_definitions_and_storage_metadata_unchanged"] = True
        except Exception as error:
            result.update(status="failed", error=str(error))
            raise
        finally:
            result["finished_at"] = dt.datetime.now(dt.timezone.utc).isoformat()
            save(evidence / "result.json", result)
            save(RUNTIME / record["run_id"] / record["template"] / "owner.json", record)
    print(f"{record['template']}: Phase 0 VM checks passed", flush=True)
    return result


def cleanup(record):
    guarded(record)
    if virsh("domstate", record["uuid"]).stdout.strip() != "shut off":
        raise ValueError("Cleanup requires a stopped clone")
    args = ["undefine", record["uuid"]]
    if len(record["paths"]) == 2:
        args.append("--keep-nvram")
    virsh(*args, log=Path(record["evidence"]) / "host.log")
    # Exact paths were checked above; never recursive-delete a runtime tree.
    for path in record["paths"]:
        Path(path).unlink()
    record["stage"] = "removed"
    save(RUNTIME / record["run_id"] / record["template"] / "owner.json", record)
    print(f"{record['template']}: removed disposable domain and copied storage; evidence retained", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["run", "prepare", "verify", "cleanup", "identity"])
    parser.add_argument("templates", nargs="+", choices=list(CONFIG["templates"]))
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,39}", args.run_id):
        parser.error("run ID must contain 1-40 lowercase letters, digits or hyphens")
    RUNTIME.mkdir(parents=True, exist_ok=True)
    with (RUNTIME / "runner.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        guestfs_setup()
        if args.action in ("run", "verify"):
            run(["bash", ROOT / "scripts/build-test-processes.sh"])
        results = []
        for key in args.templates:
            print(f"{key}: {args.action} ({args.run_id})", flush=True)
            try:
                if args.action in ("run", "prepare"):
                    record = prepare(key, args.run_id)
                else:
                    record = json.loads((RUNTIME / args.run_id / key / "owner.json").read_text())
                if args.action in ("run", "verify"):
                    results.append(verify(record))
                if args.action in ("run", "cleanup"):
                    cleanup(record)
                if args.action == "identity":
                    prepare_identity(record)
            except Exception as error:
                # Preserve preparation/cleanup errors as well as guest failures.
                destination = ROOT / "results" / args.run_id
                destination.mkdir(parents=True, exist_ok=True)
                save(destination / "failure.json", {"status": "failed", "template": key,
                     "action": args.action, "error": str(error),
                     "time": dt.datetime.now(dt.timezone.utc).isoformat()})
                save(destination / "summary.json", {"status": "failed", "error": str(error)})
                raise
        if results:
            # Include earlier individual invocations with the same run ID.
            results = [json.loads(p.read_text()) for p in sorted((ROOT / "results" / args.run_id).glob("*/result.json"))]
            if any(r["status"] != "passed" for r in results):
                raise ValueError("This run includes failed guest checks")
            if len({r["machine_id"] for r in results}) != len(results) or len({r["host_fingerprint"] for r in results}) != len(results):
                raise ValueError("Guest identities are duplicated across clones")
            save(ROOT / "results" / args.run_id / "summary.json", {"status": "passed", "results": results})


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"Phase 0 stopped: {error}", file=sys.stderr)
        sys.exit(1)
