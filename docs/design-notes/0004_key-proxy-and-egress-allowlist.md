# Design note 0004 — Key-holding sidecar proxy and default-deny egress

**Status:** proposed. Security; do before running untrusted briefs at scale.

## Problem
Keys are isolated only at container level. The host passes only the key's
path, but inside the container the key reaches Pi as an environment variable
(`src/runtime.rs` passes the variable name with `--env`;
`docker/pi/entrypoint.sh` exports it). Pi's read tool, or a manipulated
model, could read it and copy it into `outputs/` or progress lines. Egress is
unrestricted.

## Proposal
1. **Sidecar proxy holds the key.** Pi runs on an internal-only Docker
   network with a dummy key. Its provider base URL points at a sidecar that
   injects the real key (mounted read-only into the sidecar only) and forwards
   to the provider. The design container never sees the key.
2. **Default-deny egress allowlist.** The sidecar is the only route out. It
   allows the configured provider's hosts, plus hosts the run explicitly
   allows (for example a preview host such as preview.example.test, or a
   future artboard host). Everything else is refused and logged on the
   receipt.
3. **The browser renders local files only by default**
   ([0001](0001_render-look-fix-loop.md)); web fonts come through the
   allowlist only when the run allows them.

## Notes
- The provider adapter already owns hosts and env var names, so the allowlist
  can come from the adapter plus configuration.
- Test: a fixture model that tries to read the key and to reach a disallowed
  host; both fail, and the receipt says so.

## Effort
Small to medium. Preferred over adopting OpenShell
([0005](0005_openshell-verdict.md)).
