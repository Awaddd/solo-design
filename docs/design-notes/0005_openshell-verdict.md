# Design note 0005 — NVIDIA OpenShell: verdict

**Status:** decided (research 2026-11-19).

OpenShell (NVIDIA, Apache-2.0, version 0.1.x, young) offers credential
placeholders and a default-deny CONNECT proxy for agent sandboxes.

- **For the browser loop: skip.** It has no browser or computer-use support.
  Bake Chromium/Playwright into the Pi image instead
  ([0001](0001_render-look-fix-loop.md)).
- **For credentials and egress: a spike at most.** Its model fits the
  problem, but adopting it means rewriting the container runtime. Build the
  sidecar proxy first ([0004](0004_key-proxy-and-egress-allowlist.md)), and
  revisit OpenShell only if the proxy falls short or OpenShell matures.
