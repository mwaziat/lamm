# Roadmap (proposal, not promised implementation)

## Phase 0 — Baseline (this ZIP)
- [x] Rust workspace, monitor daemon, CLI, GTK interface
- [x] Linux `/proc` memory, PSI, top RSS processes
- [x] Per-user systemd service, unit tests, install/uninstall
- [ ] Locally compile/test on target Ubuntu 26.04 GNOME 50

## Phase 1 — Observability quality
- [ ] PSS rollups and process-tree attribution
- [ ] CPU, disk I/O and PSI time-series measurements
- [ ] Background collection and UI updates without main-thread blocking
- [ ] Explainable idle vs waiting vs actively-working classification

## Phase 2 — Desktop state
- [ ] Approved GNOME 50 Wayland window activity bridge
- [ ] Focused vs visible vs hidden vs minimized classification
- [ ] Multi-monitor, split-screen and multi-workspace correctness tests

## Phase 3 — Application cooperation
- [ ] Formal adapter protocol with capabilities and privacy controls
- [ ] Browser integration research: Chromium and Firefox independently
- [ ] Runtime opt-in for Node/build workers; protected defaults

## Phase 4 — Controlled optimization
- [ ] Evaluate cgroup controller ownership and conflict with systemd-oomd
- [ ] PSI-triggered opt-in policies with safety and rollback
- [ ] Benchmark RAM PSS, responsiveness, swap I/O, and wake latency

## Release prerequisites
- [ ] Linux distro compatibility checks; real automated tests
- [ ] Security review of IPC, service permissions and user data
- [ ] Distribution packaging and upgrade/migration path
