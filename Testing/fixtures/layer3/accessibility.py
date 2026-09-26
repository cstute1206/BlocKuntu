#!/usr/bin/env python3
"""Operate the installed desktop through AT-SPI, without instrumenting the app."""
import argparse
import json
import re
import time
import pyatspi

pyatspi.Atspi.set_timeout(1000, 1000)


def walk(node, depth=0):
    if depth > 30:
        return
    yield node
    try:
        children = list(node)
    except Exception:
        return
    for child in children:
        yield from walk(child, depth + 1)


def apps():
    try:
        return list(pyatspi.Registry.getDesktop(0))
    except Exception:
        # A desktop application may be registering or temporarily busy.
        # Callers use bounded polling and must still observe the target.
        return []


def app_name(application):
    try:
        return application.name
    except Exception:
        return None


def find(name, role=None, app='blockuntu-gui', seconds=15, visible=False):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        for application in apps():
            if app and app_name(application) != app:
                continue
            for node in walk(application):
                try:
                    if node.name == name and (role is None or node.getRoleName() == role):
                        if visible and not node.getState().contains(pyatspi.STATE_SHOWING):
                            continue
                        return node
                except Exception:
                    continue
        time.sleep(0.5)
    raise TimeoutError(f'Accessible control unavailable: {app}/{role}/{name}')


def click(name, app='blockuntu-gui', role='button'):
    node = find(name, role, app, visible=True)
    # Browser extension permission prompts expose the Add button before their
    # security delay expires. Invoking its action then can succeed as a no-op.
    deadline = time.monotonic() + 15
    # GTK4 exposes SENSITIVE without ENABLED for actionable chooser controls.
    while True:
        try:
            state = node.getState()
            enabled = app not in ('Firefox', 'Google Chrome') or state.contains(pyatspi.STATE_ENABLED)
            if enabled and state.contains(pyatspi.STATE_SENSITIVE):
                break
        except Exception:
            pass
        if time.monotonic() >= deadline:
            raise TimeoutError(f'Control remained disabled: {app}/{name}')
        time.sleep(0.25)
    action = node.queryAction()
    try:
        activated = action.doAction(0)
    except Exception as error:
        # Fedora's GTK file chooser can enter its nested modal loop before the
        # initiating AT-SPI D-Bus call returns. The caller must still observe
        # the requested dialog/control, so accept only this specific dispatch
        # timeout for the BlocKuntu GUI opener.
        if app == 'blockuntu-gui' and 'Did not receive a reply' in str(error):
            return
        raise
    if not activated:
        raise RuntimeError(f'Action failed: {name}')


def texts(app='blockuntu-gui'):
    out = []
    for application in apps():
        if app and app_name(application) != app:
            continue
        for node in walk(application):
            try:
                text = node.queryText()
                out.append(text.getText(0, text.characterCount))
            except Exception:
                pass
    return '\n'.join(out)


def visible_document_text(app):
    """Read the shown browser document, excluding toolbar and background tabs."""
    out = []
    for application in apps():
        if app_name(application) != app:
            continue
        for document in walk(application):
            try:
                if document.getRoleName() not in ('document web', 'document frame'):
                    continue
                if not document.getState().contains(pyatspi.STATE_SHOWING):
                    continue
                out.append(document.name)
                for node in walk(document):
                    try:
                        text = node.queryText()
                        out.append(text.getText(0, text.characterCount))
                    except Exception:
                        pass
            except Exception:
                continue
    return '\n'.join(out)


def redact(value):
    return re.sub(r'BLOCKUNTU-(?:UNINSTALL-RECOVERY|TIER1-EDIT)(?:-[A-Z0-9]+)+', '[REDACTED TEST CREDENTIAL]', value)


def dump(app=None):
    rows = []
    for application in apps():
        if app and app_name(application) != app:
            continue
        for node in walk(application):
            try:
                row = {'app': application.name, 'name': redact(node.name), 'role': node.getRoleName()}
                row['states'] = [str(state) for state in node.getState().getStates()]
                try:
                    text = node.queryText()
                    row['text'] = redact(text.getText(0, text.characterCount))
                except Exception:
                    pass
                rows.append(row)
            except Exception:
                continue
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['dump', 'click'])
    parser.add_argument('name', nargs='?')
    parser.add_argument('--app')
    args = parser.parse_args()
    if args.action == 'click':
        click(args.name, app=args.app or 'blockuntu-gui')
    else:
        print(json.dumps(dump(args.app), indent=2))
