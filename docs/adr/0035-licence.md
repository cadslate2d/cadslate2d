# ADR-0035: The repository is licensed AGPL-3.0-or-later

**Status:** Accepted (2026-10-20), legal review owed  
**Date:** 2026-10-20  
**Kind:** Governance (#292)  
**Design refs:** §44, §46

## Context

DESIGN §44 makes one hard requirement, *CadSlate2D is free to use*, and leaves open-source licensing as a separate governance decision. The project is now planned as an open application with a paid, privately held AI service (epic #291, and the private `cadslate2d/cadslate2d-ai-service` repository). The licence is what protects that model, since the application's code, including the intent IR, the constraint solver, and the generators, is open.

The choice was the maintainer's, after a discussion of the options on 2026-10-19 and 2026-10-20. Legal advice is still to be obtained (#292); this ADR records the decision as taken, and the repository stays private until the advice has been heard and the rest of #291 is done (#299).

## Decision

1. **Everything in this repository is licensed under the GNU Affero General Public License, version 3 or (at the maintainer's option in future releases, and at any recipient's option) any later version:** SPDX `AGPL-3.0-or-later`.
   - `LICENSE` at the root holds the licence text, verbatim from gnu.org.
   - Every Rust crate inherits `license = "AGPL-3.0-or-later"` from the workspace; the fuzz workspace sets it itself; `web/package.json` declares it.
   - No per-file SPDX headers are required. The manifests and `LICENSE` state the licence for the whole tree.
2. **Why the AGPL.** The application is served to browsers and may be hosted by anyone. Under the GPL, serving a modified version is not distribution, so a hosted fork could stay closed. The AGPL's §13 closes that: anyone who serves a modified version must offer its source to the people who use it. Forks stay possible, and must stay open.
3. **The private service and the copyright.** The AI service will depend on crates from this repository. The AGPL binds licensees, not the copyright holder: so long as the copyright in those crates is the maintainer's, the maintainer may use them in a closed service. That holds only while every contribution's rights are secured, which is what #294's contributor agreement (CLA) is for. Without it, an outside contribution to a crate the service uses would make the service a licensee, bound by §13.
4. **The name is not licensed.** The licence grants the code, not the trademark; #293 says what forks may call themselves.
5. **"Free to use" stands.** The licence makes the application free to use, study, change, and share; the paid service is a separate work in a separate repository.

## Alternatives considered

- **MPL-2.0.** File-level copyleft, friendlier to embedding, weaker protection: a hosted fork could add closed files around ours.
- **MIT or Apache-2.0.** Maximum adoption and no protection; a larger vendor could ship a closed derivative of the application.
- **FSL or BSL (source-available).** The strongest protection against commercial clones, converting to open source after about two years, but not open source by the OSI's definition, and a cost in goodwill the project does not need to pay.
- **AGPL-3.0-only.** Exactly version 3, with no path to a later version; the maintainer chose "or later", which also removes a friction with other "or later" code.

## Consequences

- Anyone may host CadSlate2D, modified or not; a modified hosted version must offer its source to its users.
- Dependencies must be compatible with the AGPL; #296 audits them with `cargo-deny` and a JavaScript licence checker before the repository goes public.
- The contributor agreement (#294) becomes a requirement of the business model, not a nicety.
- `README.md` and DESIGN §44 state the licence; this ADR records why.

## Revisit when

- Legal advice (#292) finds a problem with the "or later" grant, the AI-assisted authorship of most of the code, or the service's use of AGPL crates; a new ADR then supersedes this one.
- The project wants to grant a different licence to a partner, which the copyright position and the CLA would allow by a separate agreement, not by changing this one.
