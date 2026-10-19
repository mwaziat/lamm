import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class LammActivityBridge extends Extension {
    enable() {
        this._path = GLib.build_filenamev([GLib.get_user_runtime_dir(), 'lamm-activity.csv']);
        this._last = '';

        this._focusSignal = global.display.connect('notify::focus-window', () => {
            this._publish();
        });

        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
            this._publish();
            return GLib.SOURCE_CONTINUE;
        });

        this._publish();
    }

    _publish() {
        try {
            const focused = global.display.focus_window;
            const workspace = global.workspace_manager.get_active_workspace();
            const entries = new Map();

            let windows = [];
            if (typeof global.display.get_tab_list === 'function') {
                windows = global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null) || [];
            } else if (typeof global.get_window_actors === 'function') {
                windows = (global.get_window_actors() || []).map(a => a.meta_window).filter(Boolean);
            }

            for (const win of windows) {
                if (!win || win.get_window_type() !== Meta.WindowType.NORMAL)
                    continue;

                const pid = win.get_pid();
                if (pid <= 0)
                    continue;

                let state = 'BACKGROUND';
                const onActiveWorkspace = win.is_on_all_workspaces() || win.get_workspace() === workspace;

                if (win === focused) {
                    state = 'ACTIVE';
                } else if (!win.minimized && win.showing_on_its_workspace() && onActiveWorkspace) {
                    state = 'VISIBLE';
                }

                const old = entries.get(pid);
                if (!old || (state === 'ACTIVE') || (state === 'VISIBLE' && old === 'BACKGROUND')) {
                    entries.set(pid, state);
                }
            }

            // Always make sure the focused window is recorded as ACTIVE
            if (focused) {
                const focusedPid = focused.get_pid();
                if (focusedPid > 0) {
                    entries.set(focusedPid, 'ACTIVE');
                }
            }

            const lines = [`TIME,${Math.floor(GLib.get_monotonic_time() / 1000000)}`];
            for (const [pid, state] of entries) {
                lines.push(`${pid},${state}`);
            }

            const body = lines.join('\n') + '\n';
            const file = Gio.File.new_for_path(this._path);
            file.replace_contents(body, null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (error) {
            console.error(`LAMM bridge error: ${error.message}\n${error.stack}`);
        }
    }

    disable() {
        if (this._focusSignal) {
            global.display.disconnect(this._focusSignal);
            this._focusSignal = null;
        }

        if (this._timer) {
            GLib.Source.remove(this._timer);
            this._timer = null;
        }

        try {
            Gio.File.new_for_path(this._path).delete(null);
        } catch (_) {}
    }
}
