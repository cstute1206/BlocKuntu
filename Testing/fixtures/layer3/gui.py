"""Acceptance through the installed Tauri GUI and native file chooser."""
import json
import stat
import tomllib
from pathlib import Path
import pyatspi
from accessibility import app_name, apps, click, dump, find, texts, walk
from guest import WORK, host_action, rpc, run, wait
from ui_controls import focus_app, nodes, press_observed


def visible_chooser(title):
    """Return only the matching, currently showing native chooser."""
    def locate():
        for application in apps():
            name = app_name(application)
            if name in (None, '', 'gnome-shell', 'blockuntu-gui'):
                continue
            for node in walk(application):
                try:
                    role = node.getRoleName()
                    # Fedora's GTK4 Nautilus chooser exposes its root as a
                    # filler. Keep this exception scoped to Nautilus so a
                    # desktop/taskbar entry cannot be mistaken for the dialog.
                    chooser_role = role in ('dialog', 'frame', 'file chooser') or (
                        name == 'org.gnome.Nautilus' and role == 'filler')
                    if (node.name == title and chooser_role and
                            node.getState().contains(pyatspi.STATE_SHOWING)):
                        return name
                except Exception:
                    continue
        return None
    return locate()


def chooser_app(title):
    return wait(lambda: visible_chooser(title), 30)


def dismiss_welcome(guest):
    leave_desktop_overview(guest)
    focus_app()
    press_observed('Get started', lambda: not nodes(role='dialog', name='Welcome to BlocKuntu'))


def import_idle():
    """Success text can appear before the asynchronous refresh finishes."""
    return next((node for node in nodes(role='button', name='Append TOML', showing=False)
                 if node.getState().contains(pyatspi.STATE_SENSITIVE)), None)


def choose_policy_import(policy, guest, expected):
    title, stage, application = 'Append BlocKuntu policy', 'ready', None
    try:
        assert not visible_chooser(title), 'A previous import chooser is still open'
        focus_app()
        wait(import_idle, 60)
        stage = 'open-chooser'
        application = press_observed('Append TOML', lambda: visible_chooser(title))
        leave_desktop_overview(guest)
        stage = 'select-file'
        host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_L'])
        entry = chooser_editable(application)
        assert entry.queryEditableText().setTextContents(str(policy))
        host_action('keys', keys=['KEY_ENTER'])
        item = chooser_item(application, policy.name)
        assert item.parent.querySelection().selectChild(item.getIndexInParent())
        chooser_button(application, 'Select', 'Open')
        stage = 'chooser-closure'
        wait(lambda: not visible_chooser(title), 30)
        focus_app()
        stage = 'completed-import'
        wait(lambda: import_idle() and expected.lower() in texts().lower(), 60)
    except Exception as error:
        evidence = {'stage': stage, 'error': str(error), 'applications': {}}
        for app in dict.fromkeys(['blockuntu-gui', application, 'gnome-shell']):
            if app:
                try:
                    evidence['applications'][app] = dump(app)
                except Exception as capture_error:
                    evidence['applications'][app] = {'capture_error': str(capture_error)}
        (WORK / 'gui-import-failure.json').write_text(json.dumps(evidence, indent=2))
        try:
            host_action('screenshot', name='gui-import-failure')
        except Exception:
            pass
        raise


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
    """Close GNOME's overview before interacting with the GUI or a chooser."""
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
        # A short viewport can leave the button below the modal's scroll area.
        # Native focus reveals it; observe dismissal after the keyboard action.
        dismiss_welcome(guest)
        click('Settings')
        click('Notifications')
        wait(lambda: serial in texts(), 30)
        evidence['installation_id_presented'] = True
        click('Health')
        return evidence

    if not case('VM-INSTALL-004', welcome):
        raise RuntimeError('Welcome setup failed; dependent GUI cases were not executed')

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
    if not healthy:
        raise RuntimeError('GUI health failed; dependent policy cases were not executed')
    # Host has already rebooted, verified new boot ID and refreshed group membership.
    if healthy:
        def recovered():
            evidence = {'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                        'gui_and_rpc_after_reboot': True}
            if guest == 'ubuntu':
                from reboot_policy import check_process
                evidence.update(check_process())
            return evidence
        if not case('VM-INSTALL-005', recovered):
            raise RuntimeError('Post-reboot GUI/RPC failed; dependent policy cases were not executed')
    click('Rules and logging')

    def choose_import(expected='Policy appended from'):
        choose_policy_import(policy, guest, expected)

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

    if not case('VM-DATA-001', imported):
        raise RuntimeError('Policy import failed; dependent policy cases were not executed')

    def duplicate():
        before = rpc('config_snapshot')
        choose_import()
        assert rpc('config_snapshot') == before, 'Identical import changed the configuration'
        return {'unchanged_snapshot': True}
    if not case('VM-DATA-002', duplicate):
        raise RuntimeError('Duplicate import failed; dependent policy cases were not executed')

    def conflict():
        before = rpc('config_snapshot')
        policy.write_text(contents.replace('Layer 3 GUI imported rule', 'Conflicting changed name') +
                          f'\n[[rules]]\nid = "{prefix}-partial"\nname = "Must not be appended"\ntier = "hard"\npatterns = [{{kind = "domain", value = "partial.blockuntu.test"}}]\n')
        choose_import(expected='conflict')
        assert rpc('config_snapshot') == before, 'Conflict partially changed configuration'
        policy.write_text(contents)
        return {'conflict_rejected': True, 'unchanged_snapshot': True}
    if not case('VM-DATA-003', conflict):
        raise RuntimeError('Conflict import failed; dependent policy cases were not executed')

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
