import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class LammActivityBridge extends Extension {
    enable() {
        this._path = GLib.build_filenamev([GLib.get_user_runtime_dir(), 'lamm-activity.csv']);
        this._last = '';
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
            this._publish();
            return GLib.SOURCE_CONTINUE;
        });
        this._publish();
    }

    _publish() {
        const focused = global.display.focus_window;
        const workspace = global.workspace_manager.get_active_workspace();
        const entries = new Map();
        for (const actor of global.get_window_actors()) {
            const win = actor.meta_window;
            if (!win || win.get_window_type() !== Meta.WindowType.NORMAL)
                continue;
            const pid = win.get_pid();
            if (pid <= 0)
                continue;
            let state = 'BACKGROUND';
            if (win === focused)
                state = 'ACTIVE';
            else if (!win.minimized && win.showing_on_its_workspace() &&
                     (win.is_on_all_workspaces() || win.get_workspace() === workspace))
                state = 'VISIBLE';
            const old = entries.get(pid);
            if (!old || (state === 'ACTIVE') || (state === 'VISIBLE' && old === 'BACKGROUND'))
                entries.set(pid, state);
        }
        const lines = [`TIME,${Math.floor(GLib.get_monotonic_time() / 1000000)}`];
        for (const [pid, state] of entries)
            lines.push(`${pid},${state}`);
        const body = lines.join('\n') + '\n';
        // Refresh the heartbeat even if window state is unchanged.
        try {
            const file = Gio.File.new_for_path(this._path);
            file.replace_contents(body, null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (error) {
            console.warn(`LAMM bridge: ${error.message}`);
        }
    }

    disable() {
        if (this._timer) {
            GLib.Source.remove(this._timer);
            this._timer = null;
        }
        try { Gio.File.new_for_path(this._path).delete(null); } catch (_) {}
    }
}
