"""Acceptance through the installed Tauri GUI and native file chooser."""
import json
import stat
import tomllib
from pathlib import Path
from accessibility import click, dump, find, texts
from guest import WORK, host_action, rpc, run, wait


def run_cases(case, run_id):
    prefix = run_id + '-gui'
    policy = WORK / 'import.toml'
    contents = f'''[[rules]]
id = "{prefix}-exact"
name = "Layer 3 GUI imported rule"
tier = "hard"
patterns = [{{kind = "exact_url", value = "http://web.blockuntu.test:18080/exact/blocked"}}]

[[rules]]
id = "{prefix}-allow"
name = "Layer 3 inactive website allowlist"
tier = "scheduled_block"
mode = "allowlist"
patterns = [{{kind = "domain", value = "allowed.blockuntu.test"}}]
'''
    policy.write_text(contents)
    original = rpc('config_snapshot')

    def welcome():
        find('Welcome to BlocKuntu', 'dialog', seconds=60)
        evidence = {}
        for name in ['uninstall-recovery.txt', 'tier1-edit-key.txt']:
            path = Path('/etc/blockuntu') / name
            credential = path.read_text().strip()
            assert credential, f'{name} is empty'
            wait(lambda: credential in texts(), 60)
            permissions = stat.S_IMODE(path.stat().st_mode)
            assert path.stat().st_uid == 0 and permissions == 0o640
            evidence[name] = {'presented': True, 'owner_uid': 0, 'mode': oct(permissions)}
        serial = Path('/etc/blockuntu/installation-id').read_text().strip()
        assert serial, 'Installation ID is empty'
        evidence['installation_id_present'] = True
        # Never include recovery credential values in evidence.
        click('Get started')
        click('Settings')
        click('Notifications')
        wait(lambda: serial in texts(), 30)
        evidence['installation_id_presented'] = True
        click('Health')
        return evidence

    case('VM-INSTALL-004', welcome)

    def gui_health():
        click('Health')
        wait(lambda: '/run/blockuntu/blockuntud.sock mode 660' in texts() and texts().count('active\nOK') >= 5, 60)
        health_text = texts()
        assert rpc('status')['enforcement_state'] == 'active'
        (WORK / 'gui-health.json').write_text(json.dumps(dump('blockuntu-gui'), indent=2))
        host_action('screenshot', name='gui-health')
        return {'desktop_entry': 'local.blockuntu.gui', 'health_panel_visible': True,
                'health_text': health_text}

    healthy = case('VM-INSTALL-003', gui_health)
    # Host has already rebooted, verified new boot ID and refreshed group membership.
    if healthy:
        case('VM-INSTALL-005', lambda: {'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                                      'gui_and_rpc_after_reboot': True})
    click('Rules and logging')

    def choose_import(expected='Policy appended from'):
        click('Append TOML')
        find('Append BlocKuntu policy', 'frame', 'org.gnome.Nautilus')
        host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_L'])
        entry = find('', 'text', 'org.gnome.Nautilus')
        assert entry.queryEditableText().setTextContents(str(policy))
        host_action('keys', keys=['KEY_ENTER'])
        item = find('import.toml. File', 'table cell', 'org.gnome.Nautilus')
        assert item.parent.querySelection().selectChild(item.getIndexInParent())
        click('Select', app='org.gnome.Nautilus')
        wait(lambda: expected.lower() in texts().lower(), 30)

    def imported():
        choose_import()
        snapshot = rpc('config_snapshot')
        rules = snapshot['rules']
        assert sum(r['id'] == prefix + '-exact' for r in rules) == 1
        assert sum(r['id'] == prefix + '-allow' for r in rules) == 1
        assert {r['id'] for r in original['rules']} <= {r['id'] for r in rules}
        click('Close Settings')
        click('Websites')
        try:
            find('Layer 3 GUI imported rule Tier 1', 'button', visible=True)
            find('Layer 3 inactive website allowlist Tier 2 · Allowlist', 'button', visible=True)
        finally:
            click('Settings')
            click('Rules and logging')
        return {'imported_ids': [prefix + '-exact', prefix + '-allow'], 'existing_ids_preserved': True,
                'imported_rules_visible_in_gui': True}

    case('VM-DATA-001', imported)

    def duplicate():
        before = rpc('config_snapshot')
        choose_import()
        assert rpc('config_snapshot') == before, 'Identical import changed the configuration'
        return {'unchanged_snapshot': True}
    case('VM-DATA-002', duplicate)

    def conflict():
        before = rpc('config_snapshot')
        policy.write_text(contents.replace('Layer 3 GUI imported rule', 'Conflicting changed name') +
                          f'\n[[rules]]\nid = "{prefix}-partial"\nname = "Must not be appended"\ntier = "hard"\npatterns = [{{kind = "domain", value = "partial.blockuntu.test"}}]\n')
        choose_import(expected='conflict')
        assert rpc('config_snapshot') == before, 'Conflict partially changed configuration'
        policy.write_text(contents)
        return {'conflict_rejected': True, 'unchanged_snapshot': True}
    case('VM-DATA-003', conflict)

    def exported():
        basename = prefix + '-export.toml'
        target = Path.home() / basename
        assert not target.exists(), 'Export target already exists'
        click('Export TOML')
        find('Export BlocKuntu policy', 'frame', 'org.gnome.Nautilus')
        entry = find('File Name', 'text', 'org.gnome.Nautilus')
        assert entry.queryEditableText().setTextContents(basename)
        click('Save', app='org.gnome.Nautilus')
        wait(target.exists, 30)
        exported_text = target.read_text()
        config = tomllib.loads(exported_text)
        assert {prefix + '-exact', prefix + '-allow'} <= {r['id'] for r in config['rules']}
        assert any(r.get('mode') == 'allowlist' for r in config['rules'])
        for name in ['uninstall-recovery.txt', 'tier1-edit-key.txt', 'installation-id']:
            assert (Path('/etc/blockuntu') / name).read_text().strip() not in exported_text
        assert not {'service_state', 'recovery_credentials', 'usage', 'unlock_grants'} & config.keys()
        (WORK / 'export.toml').write_text(exported_text)
        host_action('screenshot', name='gui-export')
        return {'path': str(target), 'valid_toml': True, 'protected_state_omitted': True}
    case('VM-DATA-004', exported)
