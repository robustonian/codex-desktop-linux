import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const SERVICE_NAME = 'com.openai.Codex.WindowControl';
const OBJECT_PATH = '/com/openai/Codex/WindowControl';
const BACKEND = 'gnome-shell-extension';

const WINDOW_CONTROL_XML = `
<node>
  <interface name="${SERVICE_NAME}">
    <method name="ListWindows">
      <arg name="json" type="s" direction="out"/>
    </method>
    <method name="ActivateWindow">
      <arg name="window_id" type="t" direction="in"/>
      <arg name="ok" type="b" direction="out"/>
      <arg name="message" type="s" direction="out"/>
    </method>
  </interface>
</node>
`;

const WindowControlDBus = GObject.registerClass(
class WindowControlDBus extends GObject.Object {
    constructor() {
        super();

        this._dbusObject = Gio.DBusExportedObject.wrapJSObject(
            WINDOW_CONTROL_XML, this);
        this._dbusObject.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.DBus.session.own_name(
            SERVICE_NAME,
            Gio.BusNameOwnerFlags.NONE,
            null,
            () => log(`Codex Window Control lost DBus name ${SERVICE_NAME}`));
    }

    destroy() {
        if (this._nameId) {
            Gio.DBus.session.unown_name(this._nameId);
            this._nameId = 0;
        }

        this._dbusObject?.unexport();
        this._dbusObject?.run_dispose();
        this._dbusObject = null;
    }

    ListWindowsAsync(_params, invocation) {
        this._returnJson(invocation, this._listWindows());
    }

    ActivateWindowAsync([windowId], invocation) {
        const requestedId = Number(windowId);
        const window = this._listMetaWindows().find(
            candidate => Number(candidate.get_id()) === requestedId);

        if (!window) {
            invocation.return_value(new GLib.Variant('(bs)', [
                false,
                `No window matched window_id ${requestedId}`,
            ]));
            return;
        }

        try {
            if (Main.overview.visible)
                Main.overview.hide();

            if (window.minimized && typeof window.unminimize === 'function')
                window.unminimize();

            Main.activateWindow(window, global.get_current_time());
            invocation.return_value(new GLib.Variant('(bs)', [
                true,
                `Activated window_id ${requestedId}`,
            ]));
        } catch (error) {
            invocation.return_value(new GLib.Variant('(bs)', [
                false,
                `Activation failed: ${error.message}`,
            ]));
        }
    }

    _returnJson(invocation, value) {
        invocation.return_value(new GLib.Variant('(s)', [
            JSON.stringify(value),
        ]));
    }

    _listWindows() {
        return this._listMetaWindows()
            .map(window => this._windowInfo(window))
            .filter(window => window !== null);
    }

    _listMetaWindows() {
        return global.get_window_actors()
            .map(actor => actor.meta_window)
            .filter(window => window && !window.is_override_redirect?.())
            .filter(window => window.get_window_type?.() !== Meta.WindowType.DESKTOP);
    }

    _windowInfo(window) {
        if (!window)
            return null;

        const app = Shell.WindowTracker.get_default().get_window_app(window);
        const rect = window.get_frame_rect();
        const workspace = window.get_workspace?.();

        return {
            window_id: Number(window.get_id()),
            title: window.get_title?.() ?? null,
            app_id: app?.get_id?.() ?? null,
            wm_class: window.get_wm_class?.() ?? null,
            pid: window.get_pid?.() ?? null,
            bounds: rect ? {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            } : null,
            workspace: workspace?.index?.() ?? null,
            focused: global.display.focus_window === window && !Main.overview.visible,
            hidden: window.minimized ?? false,
            client_type: clientTypeName(window.get_client_type?.()),
            backend: BACKEND,
        };
    }
});

function clientTypeName(value) {
    if (value === undefined || value === null)
        return null;
    if (value === Meta.WindowClientType.WAYLAND)
        return 'wayland';
    if (value === Meta.WindowClientType.X11)
        return 'x11';
    return 'unknown';
}

export default class CodexWindowControlExtension extends Extension {
    enable() {
        this._dbusServer = new WindowControlDBus();
    }

    disable() {
        this._dbusServer?.destroy();
        this._dbusServer = null;
    }
}
