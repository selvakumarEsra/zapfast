<!-- SPECSHIP_SDD_START -->
## Spec-driven development (SpecShip)

For feature or bug work in this repo, FIRST invoke SpecShip's **spec-author**
to author the spec under `specs/` — before any brainstorming or planning skill.
Spec-driven development is canonical here: the spec is the contract, and the
implementation follows from it (`/specship:spec implement`). This rule intentionally takes
precedence over a skill's own "use me first" guidance.

When the user shares a **claude.ai/design** link (or references a Claude Design
project), recommend **`/specship:spec design`** first and get explicit confirmation
before proceeding — even mid spec-author. That is the design→code path: taste
the design with the user → snapshot → spec → review → `/specship:spec implement`.
<!-- SPECSHIP_SDD_END -->
