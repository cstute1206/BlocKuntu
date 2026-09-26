"""Acceptance through the installed Tauri GUI and native file chooser."""
import json
import stat
import tomllib
from pathlib import Path
import pyatspi
from accessibility import app_name, apps, click, dump, find, texts, walk
from guest import WORK, host_action, rpc, run, wait


def chooser_app(title):
    """Return the native chooser's AT-SPI application across GNOME/KDE."""
    def locate():
        for application in apps():
            name = app_name(application)
            if name in (None, '', 'gnome-shell', 'blockuntu-gui'):
                continue
            for node in walk(application):
                try:
                    if (node.name == title and node.getRoleName() in ('dialog', 'frame', 'file chooser') and
                            node.getState().contains(pyatspi.STATE_SHOWING)):
                        return name
                except Exception:
                    continue
        return None
    return wait(locate, 30)


def chooser_editable(application):
    def locate():
        for candidate in apps():
            if app_name(candidate) != application:
                continue
            for node in walk(candidate):
                try:
                    if not node.getState().contains(pyatspi.STATE_SHOWING):
                        continue
                    node.queryEditableText()
                    return node
                except Exception:
                    continue
        return None
    return wait(locate, 15)


def chooser_filename_editable(application):
    """Select KDE's filename field, not its earlier editable location bar."""
    def locate():
        for candidate in apps():
            if app_name(candidate) != application:
                continue
            for label in walk(candidate):
                try:
                    if (label.name != 'Name:' or label.getRoleName() != 'label' or
                            not label.getState().contains(pyatspi.STATE_SHOWING)):
                        continue
                    field = label.parent[label.getIndexInParent() + 1]
                    for node in walk(field):
                        try:
                            if node.getState().contains(pyatspi.STATE_SHOWING):
                                node.queryEditableText()
                                return node
                        except Exception:
                            continue
                except Exception:
                    continue
        return None
    return wait(locate, 15)


def chooser_button(application, *names):
    for name in names:
        try:
            find(name, 'button', application, seconds=2, visible=True)
        except TimeoutError:
            continue
        click(name, app=application)
        return
    raise TimeoutError(f'Native chooser action unavailable: {application}/{names}')


def leave_desktop_overview(guest):
    """Close GNOME's overview if a modal chooser dispatch exposed it."""
    if guest == 'fedora':
        # Fedora 44 can expose the first chooser in GNOME's overview, while
        # later choosers open directly. Esc is only safe in the overview.
        try:
            find('Overview', 'panel', app='gnome-shell', seconds=1, visible=True)
        except TimeoutError:
            return
        host_action('keys', keys=['KEY_ESC'])
        try:
            find('Overview', 'panel', app='gnome-shell', seconds=1, visible=True)
        except TimeoutError:
            return
        raise RuntimeError('Fedora desktop overview remained open above the native chooser')
    for _ in range(3):
        try:
            find('Type to search', app='gnome-shell', seconds=1, visible=True)
        except TimeoutError:
            return
        host_action('keys', keys=['KEY_ESC'])
    try:
        find('Type to search', app='gnome-shell', seconds=1, visible=True)
    except TimeoutError:
        return
    raise RuntimeError('Desktop overview remained open above the native chooser')


def chooser_item(application, basename):
    def locate():
        for candidate in apps():
            if app_name(candidate) != application:
                continue
            for node in walk(candidate):
                try:
                    if node.name in (basename, basename + '. File'):
                        return node
                except Exception:
                    continue
        return None
    return wait(locate, 15)


def run_cases(case, run_id, guest='ubuntu'):
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
        dialog_app = chooser_app('Append BlocKuntu policy')
        leave_desktop_overview(guest)
        host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_L'])
        entry = chooser_editable(dialog_app)
        assert entry.queryEditableText().setTextContents(str(policy))
        host_action('keys', keys=['KEY_ENTER'])
        item = chooser_item(dialog_app, 'import.toml')
        assert item.parent.querySelection().selectChild(item.getIndexInParent())
        chooser_button(dialog_app, 'Select', 'Open')
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
        dialog_app = chooser_app('Export BlocKuntu policy')
        leave_desktop_overview(guest)
        entry = (chooser_filename_editable(dialog_app) if guest == 'cachyos'
                 else chooser_editable(dialog_app))
        filename = str(target) if guest == 'cachyos' else basename
        assert entry.queryEditableText().setTextContents(filename)
        chooser_button(dialog_app, 'Save')
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
