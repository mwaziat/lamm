# Architecture contract — v0.1

## Product boundaries

Single installable product: resident unprivileged daemon + lightweight GNOME UI + CLI. Background operation independent of UI lifecycle. Explicitly separate observation from policy execution.

## OS layer

Read `/proc` and PSI for baseline telemetry. Future OS control could leverage systemd-managed cgroups v2 and kernel reclaim. Do not directly manipulate another process's `/proc/<pid>/mem`, inject shared libraries or use kernel hacks. No privilege escalation in v0.1.

## Application layer (future)

A generic adapter protocol may report active/visible/hidden/idle status and explicit safe actions; browser implementations must use supported integration paths, respecting permissions and user privacy. A single host adapter is not a guarantee that Chrome and Firefox expose identical lifecycle actions. Runtime workloads must opt in before being suspended.

## Desktop compositor

On GNOME Wayland, focus/visibility are compositor-level facts. A GNOME integration may report visible windows and workspace state. Split-screen and multiple monitors must be treated as multiple visible surfaces. Focus alone is insufficient.

## Process accounting

RSS can double count shared memory; prefer PSS for per-process attribution in the next phase. Avoid assuming a 1:1 mapping between browser tabs and Linux PIDs, or between child process and window.

## Safety invariants

1. Observe-only until policies are explicitly designed and tested.
2. Never kill or suspend foreground/visible processes automatically.
3. Critical system services, compositor, data stores and build/runtime tasks protected by default.
4. No broad `drop_caches`, `swapoff -a`, swappiness changes or OOM pressure tricks as optimization shortcuts.
5. Rollback configuration after crashes; policy execution needs auditable logs, opt-in and limits.
6. Prefer measuring responsiveness, faults, PSI, and resume latency to maximizing free RAM.

## Potential future control plane

A user agent can request documented privileged operations through a tightly scoped privileged service. The privileged boundary and authorization model must be reviewed before implementation. No unrestricted privileged daemon by default.
