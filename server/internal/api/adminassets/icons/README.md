# Bundled website marks

These assets identify existing destinations in NodeCloak's site picker. They are
bundled in both the desktop and server, and are never downloaded from a third-party
icon service at runtime. All SVGs use a reviewed allowlist of SVG geometry and
contain no script, event handler, external reference, CSS, or foreign object.
Brand names and marks remain the property of their respective owners.

| File | Source | Asset license |
| --- | --- | --- |
| claude.svg | [Simple Icons 16.4.0](https://github.com/simple-icons/simple-icons/tree/16.4.0), `claude` | CC0 (bundled license) |
| chatgpt.svg | [Simple Icons 14.15.0](https://github.com/simple-icons/simple-icons/tree/14.15.0), `openai` | CC0 (bundled license) |
| gemini.svg | Simple Icons 16.4.0, `googlegemini` | CC0 |
| amazon.svg | Simple Icons 14.15.0, `amazon` | CC0 |
| ebay.svg | Simple Icons 16.4.0, `ebay` | CC0 |
| etsy.svg | Simple Icons 16.4.0, `etsy` | CC0 |
| walmart.svg | Simple Icons 14.15.0, `walmart` | CC0 |
| shopee.svg | Simple Icons 16.4.0, `shopee` | CC0 |
| grok.svg | [Lobe Icons 1.74.0](https://github.com/lobehub/lobe-icons), XAI/Mono (title Grok) | MIT, copyright 2023 LobeHub (bundled license) |
| lazada.svg | [Lawnicons Lazada logomark](https://github.com/lawnchairlauncher/lawnicons/blob/main/svgs/lazada.svg) | Apache 2.0 (bundled license) |
| muse.svg | NodeCloak provisional M monogram | Project MIT |
| ai.svg, shopping.svg, globe.svg | NodeCloak basic category geometry | Project MIT |

Simple Icons paths are preserved, with explicit brand-colored fills added for
standalone image rendering. The Grok path is preserved with a neutral fill. The
Lazada path is preserved, with its stroke set to pink and the viewport normalized.

The old shortcut labels “Muse” and points to `https://muse.ai/`. During this work,
that address did not provide a reliably identifiable brand asset. The bundled
Muse M monogram is therefore a **provisional label mark**, not a claimed official
Muse logo. Its destination and icon are editable through the administration
catalog; an administrator can upload the correct normalized brand image.

Third-party source licenses are stored beside this file. Only the 14 explicitly
allowed `.svg` names are served by the icon endpoint; these documents are not
public icon endpoints.
