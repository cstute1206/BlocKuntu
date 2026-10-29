"""Native AT-SPI form operations for installed-product acceptance."""
import pyatspi
from accessibility import apps, app_name, click, find, walk
from guest import host_action, wait


def nodes(app='blockuntu-gui', role=None, name=None, showing=True):
    found = []
    for application in apps():
        if app_name(application) != app:
            continue
        for node in walk(application):
            try:
                if ((not showing or node.getState().contains(pyatspi.STATE_SHOWING)) and
                        (role is None or node.getRoleName() == role) and
                        (name is None or node.name == name)):
                    found.append(node)
            except Exception:
                pass
    return found


def enabled(node):
    return node.getState().contains(pyatspi.STATE_SENSITIVE)


def activate(node):
    assert enabled(node), 'Control is disabled'
    assert node.queryAction().doAction(0), 'Control action failed'


def field(label):
    def locate():
        for node in nodes(role='entry',showing=False):
            try:
                if node.name!=label and not node.name.startswith(label+' '):
                    continue
                return node
            except Exception:
                pass
        return None
    return wait(locate, 15)


def edit(node, value):
    assert enabled(node), 'Text field is disabled'
    assert node.queryComponent().grabFocus()
    wait(lambda: node.getState().contains(pyatspi.STATE_FOCUSED) and
         node.getState().contains(pyatspi.STATE_SHOWING), 15)
    # WebKit exposes HTML inputs through Text/Component but not EditableText.
    host_action('type_form', value=value)
    wait(lambda:node.queryText().getText(0,node.queryText().characterCount)==value,15)


def choose(node, label):
    assert enabled(node), 'Select is disabled'
    options = [child for child in walk(node) if child.getRoleName() in ('menu item', 'list item')]
    labels = [child.name for child in options]
    assert label in labels, ('Select option unavailable', node.name, labels, label)
    def selected():
        return [child.name for child in walk(node)
                if child.getRoleName() in ('menu item', 'list item') and
                child.getState().contains(pyatspi.STATE_SELECTED)]
    def expanded():
        return node.getState().contains(pyatspi.STATE_EXPANDED)
    if selected() == [label]:
        if expanded():
            host_action('keys', keys=['KEY_ESC'])
            wait(lambda: not expanded() and selected() == [label], 15)
        return
    assert node.queryComponent().grabFocus()
    wait(lambda: node.getState().contains(pyatspi.STATE_FOCUSED) and
         node.getState().contains(pyatspi.STATE_SHOWING), 15)
    # Closed HTML selects handle navigation keys differently across WebKit/GTK
    # versions. Open the native popup explicitly before navigating its options.
    if not expanded():
        host_action('keys', keys=['KEY_SPACE'])
        wait(expanded, 15)
    host_action('keys', keys=['KEY_HOME'])
    for _ in range(labels.index(label)):
        host_action('keys', keys=['KEY_DOWN'])
    host_action('keys', keys=['KEY_ENTER'])
    # An acknowledged key dispatch does not prove the popup released its grab.
    # Typing into the next field while it is open can go to the menu instead.
    wait(lambda: not expanded() and selected() == [label], 15)
    host_action('keys', keys=['KEY_TAB'])


def combo(label):
    return wait(lambda:next((n for n in nodes(role='combo box',showing=False)
        if n.name==label or n.name.startswith(label+' ')),None),15)


def focus_app(app='blockuntu-gui'):
    frames=wait(lambda:nodes(app=app,role='frame'),15)
    # A modal close or native app switch can replace the accessible frame.
    # Requery the desktop instead of polling the original frame indefinitely.
    def active():return any(n.getState().contains(pyatspi.STATE_ACTIVE)
        for n in nodes(app=app,role='frame'))
    if active():return
    # GNOME can leave a newly launched window visible inside its overview.
    # Exit that shell surface before attempting native application switching.
    host_action('keys',keys=['KEY_ESC'])
    try:
        wait(active,1)
        return
    except TimeoutError:
        pass
    try:
        frames[0].queryComponent().grabFocus()
        wait(active,1)
    except Exception:
        host_action('keys',keys=['KEY_LEFTALT','KEY_TAB'])
    try:
        wait(active,5)
    except TimeoutError:
        host_action('keys',keys=['KEY_LEFTALT','KEY_TAB'])
        wait(active,10)


def open_settings(section):
    focus_app()
    if not nodes(role='dialog',name='Settings'):
        click('Settings')
    click(section)


def press(name):
    node=find(name,'button',visible=False)
    assert enabled(node),'Button is disabled: '+name
    assert node.queryComponent().grabFocus(),'Button did not receive focus: '+name
    wait(lambda:node.getState().contains(pyatspi.STATE_SHOWING),15)
    # WebKit can dispatch the click but time out its synchronous AT-SPI reply.
    # Use the focused button's native keyboard action; callers observe its effect.
    host_action('keys',keys=['KEY_ENTER'])


def press_observed(name, ready):
    """Accept a dispatch timeout only when its expected UI change is observed."""
    try:
        press(name)
    except Exception as error:
        if 'Did not receive a reply' not in str(error):
            raise
    return wait(ready, 15)


def entries(table=False,showing=True):
    found=[]
    for node in nodes(role='entry',showing=showing):
        try:
            if table:
                ancestor=node.parent
                while ancestor and ancestor.getRoleName() not in ('table', 'application'):
                    ancestor=ancestor.parent
                if not ancestor or ancestor.getRoleName()!='table':
                    continue
            found.append(node)
        except Exception:
            pass
    return found
