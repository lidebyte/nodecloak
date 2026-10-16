# Firefox startup context selection

## Report and reproduction

A macOS user on NodeCloak 3.1.1 reported a Firefox window appearing then closing,
with `script.evaluate (unsupported operation)`. Firefox's version was unavailable.
A subsequent attempt showed Firefox's startup troubleshooting dialog and the
generic local-control-endpoint timeout. These screenshots do not by themselves
establish a native Firefox crash or its cause.

The same script rejection was reproduced on installed Firefox 158 on Windows
using a fresh temporary profile launched on `about:home`. The previous initializer
unconditionally selected the first top-level browsing context and evaluated the
browser-network region lookup there. Firefox internal pages restrict script
access. The error propagated to the existing profile startup cleanup, which
closed the newly launched browser. A normal blank-page IP launch passed before
the change.

Mozilla's relevant security model is documented at:
https://firefox-source-docs.mozilla.org/remote/Security.html

## Change

Select an existing default-user-context startup `about:blank` tab, or create one when the current
tabs are internal/restored pages. Explicitly navigate that tab to `about:blank`
and await completion before any region query. Keep the original tabs intact.
The query still runs inside the actual profile browser and uses its routing.
Initialization failures remain errors rather than silently dropping regional
settings. No system-access permission, profile reset or new mode is introduced.

## Verification

- Installed Firefox protected-page regression failed with the reported protocol
  error before the change, then passed after the change. It also checks that the
  original home tab remains present.
- Protocol regression covers an internal page and restored website preceding a
  blank tab, and creation when no blank tab exists; evaluation must wait for
  navigation and must target the startup blank tab. A restored container's blank
  is never reused because regional settings and preloads apply to the default
  user context.
- Normal installed Firefox IP-region initialization/retry and provider restoration
  are checked by the existing opt-in integration regression.

Local tests exercise the shared startup path on Windows. The 3.1.2 release
matrix additionally installs Firefox and runs both native startup regressions
on macOS ARM64 and Intel before packaging. The reporter's exact Firefox build
remains unavailable; release execution and package verification are recorded
in the release plan.
