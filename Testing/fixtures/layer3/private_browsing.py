"""Regular/private-window behavior and Chromium setting locks in native browsers."""
import datetime as dt
import json
from pathlib import Path
import time
import pyatspi

from accessibility import click, dump, walk
from browsers import APPS, BROWSER_CONFIG, start, stop, navigate as browser_navigate, heartbeat
from guest import WORK, host_action, rpc, wait
from ui_controls import activate, choose, enabled, focus_app, nodes, open_settings


def editable_private_settings():
    # Natural expiry and the GUI's periodic refresh are separate observations.
    wait(lambda:rpc('tier1_edit_status')['chromium_incognito_settings_change_allowed'] is True,30)
    open_settings('Protected changes and uninstall')
    # Requery because a refresh may replace accessible nodes.
    return wait(lambda:next((n for n in nodes(role='combo box') if
        n.name.startswith('Chromium private browsing') and enabled(n)),None),30)


def incognito_consent_enabled(node):
    state=node.getState()
    # Chrome 151 exposes its aria-pressed control as a toggle button; newer
    # versions expose a checked control. Focus or successful dispatch is not consent.
    return state.contains(pyatspi.STATE_CHECKED) or (
        node.getRoleName()=='toggle button' and state.contains(pyatspi.STATE_PRESSED))


def run_cases(browser,profile,run_id,case):
    app=APPS[browser]
    outside='http://web.blockuntu.test:18080/outside'
    allowed='http://web.blockuntu.test:18080/free'
    counter=0
    current_window=None
    regular_window=None

    def browser_windows():
        # Chrome also exposes unnamed auxiliary frames as SHOWING. They are
        # not browser windows and must not affect window creation assertions.
        return [frame for frame in nodes(app=app,role='frame') if frame.name.strip() and any(
            document.getRoleName() in ('document web','document frame') and
            document.getState().contains(pyatspi.STATE_SHOWING) for document in walk(frame))]

    def document_text(window):
        out=[]
        for document in walk(window):
            if document.getRoleName() not in ('document web','document frame') or not document.getState().contains(pyatspi.STATE_SHOWING):
                continue
            out.append(document.name)
            for node in walk(document):
                try:
                    text=node.queryText()
                    out.append(text.getText(0,text.characterCount))
                except Exception:pass
        return '\n'.join(out)

    def page(_browser,text,seconds=45):
        nonlocal current_window
        if current_window is None:
            frames=wait(browser_windows,15)
            assert len(frames)==1,'Expected one regular browser window'
            current_window=frames[0]
        wait(lambda:text in document_text(current_window),seconds)

    def navigate(browser,url):
        if current_window is None:
            focus_app(app)
        browser_navigate(browser,url,window=current_window)

    def settings():
        open_settings('Protected changes and uninstall')
        return wait(lambda:next((n for n in nodes(role='combo box') if n.name.startswith('Chromium private browsing')),None),15)

    def mode(value):
        selector=editable_private_settings()
        choose(selector,{'disabled':'Disable private browsing (recommended for full allowlist protection)',
                         'manual_consent':'Allow with manual extension consent'}[value])
        wait(lambda:rpc('tier1_edit_status')['chromium_incognito_mode']==value,30)
        click('Close Settings')

    def activate_allowlist():
        nonlocal counter
        counter+=1
        rule={'id':run_id+'-'+browser+'-private-'+str(counter),
              'name':'Acceptance private allowlist '+browser+' '+str(counter),
              'tier':'scheduled_block','mode':'allowlist','patterns':[{'kind':'exact_url','value':allowed}]}
        rpc('upsert_site_list',{'rule':rule})
        session=rpc('start_detox',{'name':rule['name'],'duration_minutes':1,'site_rule_ids':[rule['id']]})['session']
        return rule,session

    def ended(session):
        end=dt.datetime.fromisoformat(session['ends_at'].replace('Z','+00:00'))
        wait(lambda:dt.datetime.now(dt.timezone.utc)>=end,max(1,(end-dt.datetime.now(dt.timezone.utc)).total_seconds())+5)

    def named_block(rule):
        navigate(browser,outside)
        page(browser,'Blocked by BlocKuntu')
        assert rule['name'] in document_text(current_window),'Wrong private-window rule'
        navigate(browser,allowed)
        page(browser,'BlocKuntu test route /free')
        navigate(browser,outside)
        page(browser,'Blocked by BlocKuntu')

    def record(scenario,evidence):
        for cid in ('VM-BR-004','VM-WAL-013'):
            host_action('screenshot',name=browser+'-'+cid.lower()+'-'+scenario)
            assert case(cid,lambda:evidence,scenario=scenario),'Private-browser prerequisite failed'

    def open_private():
        nonlocal current_window,regular_window
        before=browser_windows()
        regular_window=current_window
        focus_app(app)
        host_action('keys',keys=['KEY_LEFTCTRL','KEY_LEFTSHIFT','KEY_P' if browser=='firefox' else 'KEY_N'])
        def created():
            added=[n for n in browser_windows() if n not in before]
            return added[0] if len(added)==1 else None
        current_window=wait(created,30)
        wait(lambda:('private' if browser=='firefox' else 'incognito') in document_text(current_window).lower(),30)
        return {'private_window_opened':True,'private_start_page_observed':True,
                'private_document_verified':True}

    def close_private():
        nonlocal current_window
        closing=current_window
        host_action('keys',keys=['KEY_LEFTALT','KEY_F4'])
        wait(lambda:closing not in browser_windows(),15)
        current_window=regular_window

    def locked():
        selector=settings()
        wait(lambda:not enabled(selector),30)
        scopes=[n for n in nodes(role='combo box') if n.name.startswith('When to disable private browsing')]
        assert all(not enabled(n) for n in scopes)
        before=rpc('tier1_edit_status')
        for method,params in [('set_chromium_incognito_mode',{'mode':'manual_consent' if before['chromium_incognito_mode']=='disabled' else 'disabled'}),
                              ('set_chromium_incognito_disable_scope',{'scope':'active_schedule_or_detox'})]:
            try:rpc(method,params)
            except RuntimeError as error:assert 'allowlist' in str(error).lower(),str(error)
            else:raise AssertionError('Daemon allowed private-setting changes under active allowlist')
        after=rpc('tier1_edit_status')
        assert before['chromium_incognito_mode']==after['chromium_incognito_mode']
        assert before['chromium_incognito_disable_scope']==after['chromium_incognito_disable_scope']
        host_action('screenshot',name='gui-'+browser+'-private-settings-locked-'+str(counter))
        click('Close Settings')
        return {'gui_settings_locked':True,'daemon_settings_rejected':True,'settings_unchanged':True}

    try:
        if browser=='firefox':
            policy=json.loads(Path(BROWSER_CONFIG[browser]['policy']).read_text())['policies']
            ext=heartbeat(browser)['extension_id']
            assert policy['ExtensionSettings'][ext]['private_browsing'] is True
            assert policy['PrivateBrowsingModeAvailability']==0
            rule,session=activate_allowlist()
            named_block(rule)
            record('regular',{'rule_id':rule['id'],'regular_blocked':True,'allowed_access':True,'policy_private_extension_permitted':True})
            evidence=open_private()
            named_block(rule)
            record('private',{'rule_id':rule['id'],'private_blocked':True,'allowed_access':True,'policy_private_extension_permitted':True,**evidence})
            close_private()
            ended(session)
            return

        # Native Chrome/Chromium first demonstrate the manual-consent gap.
        assert rpc('tier1_edit_status')['chromium_incognito_mode']=='manual_consent'
        rule,session=activate_allowlist()
        named_block(rule)
        record('regular',{'rule_id':rule['id'],'regular_blocked':True,'allowed_access':True,'configured_mode':'manual_consent'})
        lock=locked()
        evidence=open_private()
        navigate(browser,outside);page(browser,'BlocKuntu test route /outside')
        record('incognito-configured-unprotected',{'rule_id':rule['id'],'private_unprotected_as_configured':True,
            'configured_mode':'manual_consent','explicit_extension_consent':False,**lock,**evidence})
        close_private()
        ended(session)

        # Give consent through the real browser-owned extension settings page.
        extension_id=wait(lambda:heartbeat(browser),30)['extension_id']
        navigate(browser,'chrome://extensions/?id='+extension_id)
        toggle=wait(lambda:next((n for n in nodes(app=app) if n.name.startswith('Allow in Incognito') and n.getRoleName() in
                    ('check box','toggle button','switch')),None),30)
        assert not incognito_consent_enabled(toggle),'Fresh profile already consented'
        activate(toggle)
        wait(lambda:incognito_consent_enabled(toggle),15)
        host_action('screenshot',name=browser+'-private-extension-consent')
        # Chrome's control explicitly says this preference applies on restart.
        # Keep the browser absent for a scan, then observe a new native session.
        old_session=heartbeat(browser)['session_started_at']
        stop(browser)
        current_window=None
        component=BROWSER_CONFIG[browser]['component']
        wait(lambda:not rpc('extension_status',{'component':component})['browser_running'],30)
        time.sleep(15)
        start(browser,profile,[allowed])
        wait(lambda:heartbeat(browser) and heartbeat(browser)['session_started_at']!=old_session,30)
        navigate(browser,allowed);page(browser,'BlocKuntu test route /free')
        rule,session=activate_allowlist()
        evidence=open_private();named_block(rule)
        record('incognito-extension-permitted',{'rule_id':rule['id'],'private_blocked':True,'allowed_access':True,
            'explicit_extension_consent':True,'consent_browser_restarted':True,'configured_mode':'manual_consent',**evidence})
        close_private()
        ended(session)

        stop(browser)
        current_window=None
        mode('disabled')
        start(browser,profile,[allowed]);wait(lambda:heartbeat(browser),30);page(browser,'BlocKuntu test route /free')
        rule,session=activate_allowlist()
        lock=locked();named_block(rule)
        policy=json.loads(Path(BROWSER_CONFIG[browser]['policy']).read_text())
        assert policy['IncognitoModeAvailability']==1
        before=len(browser_windows())
        host_action('keys',keys=['KEY_LEFTCTRL','KEY_LEFTSHIFT','KEY_N'])
        time.sleep(3)
        assert len(browser_windows())==before and 'Blocked by BlocKuntu' in document_text(current_window)
        record('incognito-disabled',{'rule_id':rule['id'],'private_creation_prevented':True,'regular_blocked':True,
            'configured_mode':'disabled','policy_incognito_mode':1,**lock})
        ended(session)
        stop(browser);mode('manual_consent')
        current_window=None
        start(browser,profile,[allowed]);wait(lambda:heartbeat(browser),30);page(browser,'BlocKuntu test route /free')
    except Exception:
        (WORK/'private-failure.json').write_text(json.dumps(dump(app),indent=2))
        host_action('screenshot',name=browser+'-private-failure')
        raise
