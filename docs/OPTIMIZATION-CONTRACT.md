# LAMM — Optimization Contract (working proposal, not yet implemented)

## Product goal
LAMM is one Linux desktop product with a small UI and continuously running service. **Its purpose is optimizing interactive responsiveness and effective memory usage**, not merely displaying system metrics.

Two cooperating planes:

1. **OS-level optimization**: manage groups of processes using existing Linux mechanisms (systemd user scopes / cgroups v2, CPU weights and memory protection, pressure-aware reclaim). Observe PSI and usage trends to decide when interventions are justified. Do not build an independent page allocator or flush global caches.
2. **Application-level optimization**: exchange lifecycle hints with applications via opt-in integration (e.g. browser extension and Native Messaging) so a background tab can be discarded or frozen by the browser itself, rather than blindly SIGSTOPing renderer PIDs. Application integrations are modular, but ship as one installable LAMM product.

## Visibility model
- `CRITICAL`: kernel/system services, compositor and protected user workloads; no automatic reclaim or freeze.
- `FOCUSED`: window receiving input; strongest interactive protection.
- `VISIBLE`: split-screen, other visible windows, second monitor; interactive protection even without focus.
- `BACKGROUND`: obscured/minimized/off-workspace, but possibly still working; conservative handling.
- `IDLE_CANDIDATE`: not visible for >=30 minutes **and** not covered by a blocker; eligible for further evaluation, not automatically frozen.

A window can be visible without focus. An app can have multiple windows. One browser window can contain multiple tabs; compositor visibility does not reveal tab lifecycle. Screen locking is not equivalent to all workloads becoming idle.

## OS decision model
- Inputs: GNOME Shell window visibility/focus events (session component); systemd cgroup membership; PSI `/proc/pressure/{memory,cpu,io}`; available RAM; swap traffic; service protection/blockers.
- Trigger: **pressure-based**, not 'RAM used high' alone. Reclaim only when pressure threshold is sustained, with cooldown and hysteresis.
- Normal state: protect focused + visible processes from relative contention where allowed; leave background work running.
- Elevated pressure: prefer safe, application-provided release mechanisms first; use kernel cgroup reclaim on eligible inactive scopes only when supported; optionally reduce CPU weight for deprioritized background scopes.
- Recovery: restore weights/protection immediately when window becomes visible; do not terminate or freeze processes by default.
- Preserve systemd-oomd semantics and avoid conflicting writes to cgroups that systemd owns. Scoped control requires careful handoff via systemd D-Bus.

## Application decision model
- Browser cooperation should report tab active/visible, time since last interaction, audio/video, downloads, WebRTC, unsaved forms, service workers and other blockers where observable.
- For tabs not visible >=30 min, request *browser-supported* freeze/discard only if compatible and safe; browser remains authority and can refuse. No claim that all state survives discard.
- Runtime workloads (Node.js, database, build, socket services): detect ongoing work but never suspend automatically until opt-in and safe lifecycle contract exist.
- Unknown applications fall back to OS-level prioritization only; do not pretend to know internal tab/task states.

## Safety and performance
- Default no root, no global `drop_caches`, no `swapoff -a`, no blind SIGSTOP/SIGKILL, no blanket memory.max on active apps.
- Protect GNOME Shell/Mutter, active developer processes, critical user tasks and data integrity.
- Changes should be reversible, rate-limited, traceable; fail open (leave applications functioning) if desktop integration disappears.
- Benchmark p95/p99 input latency and resume latency alongside PSI, PSS, CPU, swap-in/out; improvements are not guaranteed.

## Implementation status as of v0.1.0
- Implemented: sample Linux telemetry, dashboard, daemon/socket.
- **Not implemented**: GNOME visibility connector, safe scope ownership, cgroup policy enforcement, browser native messaging, browser tab lifecycle commands, pressure controller and rollback.
- Do NOT describe the current ZIP as an optimizing tool. It is an incomplete prototype and cannot fulfill the primary product goal.

## Minimum meaningful next milestone
1. GNOME session component emits focused/visible/background app identities for Wayland (multi-monitor/split-view included).
2. Daemon maps identities to systemd-owned scopes and simulates deterministic policy decisions with safety exemptions.
3. Implement an **opt-in and rollbackable** cgroup adjustment on a safe test scope with measured impact.
4. Browser adapter v1 emits tab activity and blockers, then calls only browser-supported actions on consent.
5. Validate under high memory pressure; compare baseline with and without LAMM.
