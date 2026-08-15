# Verity desktop design baseline

Verity is a single-task verification tool, not a dashboard.

- Three primary destinations only: Current check, History, Settings.
- One continuous work plane. No card grids, graphs, or embedded terminal simulations.
- Cold near-black surfaces, low-noise dividers, electric violet for active controls, and semantic green/amber/red for result state. Bronze belongs only to the shield identity.
- Paths, commands, identifiers, versions, and hashes remain untranslated and monospace.
- Every action has a stable `data-action-id` and exposes ready, pending, disabled, result, or error state within 100 ms.
- Disabled actions remain focusable through `aria-disabled` and expose their reason.
- Control response is 120-180 ms. Stage changes and receipt completion are 220-300 ms. Animation uses transform and opacity only.
- Reduced motion removes displacement, shimmer, and persistent animation.
- Validate English and Chinese at 1100x640 through 1440x1024, 115% scale, keyboard-only operation, long output, and all session states.

The material direction may borrow the fixed narrow rail, quiet dark surface, single primary action, and fine lighting of Unicorn Studio. It must not copy template-card waterfalls or add decorative motion to execution surfaces.

## Current check evidence field

- The current repository, plan, target, session, and receipt belong to the application workspace, not to a routed page. Navigation never resets an active check.
- The last repository path may be restored on launch for inspection only. Restoration never creates or executes a run session.
- Six visible stages organize the current check: detect, plan, dependencies, build, test and launch, and Oracle verification.
- The selected production composition is a curved six-stage path on the left, a fixed evidence rail on the right, and a session action bar at the bottom. The evidence rail is the current page's only scrolling region.
- Stage nodes are semantic DOM buttons. The SVG curve and elastic evidence field are progressive visual layers and never own interaction or status truth.
- OGL renders a fixed-seed, lightly disturbed 64 by 42 graphite point field and its Delaunay triangle topology. Deterministic broad ridges, valleys, slope lighting, density changes, and fog project the same topology into a static terrain; the fallback SVG uses the identical projected coordinates. Motion handles interruptible path, node, evidence, and bottom-bar state transitions. ReactBits is reference material only and contributes no source code.
- The evidence field reacts to stage activation, real session heartbeat, first blocker, verified completion, and fine-pointer hover. Hover is a bounded lens over the existing topology; it never emits particles or becomes a perpetual background animation.
- The material stack is bright graphite points, extremely low-contrast global triangle edges, and a thresholded nine-sample glow pass. It contains no detached event particles or short fiber geometry. Active phases retain static violet density between real heartbeats; blocked and verified states use amber and green only.
- Pointer down and Enter or Space activation enter the field synchronously; initialization may queue, but never discard, the first selection event. Each activation produces one interruptible 720 ms gravitational response: 90 ms inward convergence, two phase-offset topology waves within a 140 px radius, then elastic recovery. A later activation cancels the old response instead of stacking another wave.
- Stage paths have an opaque graphite body, a narrow edge highlight, a semantic progress layer, and a shallow shadow. Every stage control uses the same restrained black-titanium bezel, smoked lens, inset core, and at most two state rings. The activity marker never replaces the stage number.
- The current repository row, evidence rail, and session bar may use fixed noise, graphite fog, inner highlights, and hairline separators. They remain parts of one continuous work plane and must not become floating cards.
- Material quality is mount-local and only degrades: `full` uses points, triangle topology, bounded hover, and thresholded glow with DPR at most 1.5; `compact` removes glow and caps DPR at 1; `static` uses an SVG generated from the same fixed topology. The first meaningful motion samples 36 frames and degrades when render P95 exceeds 20 ms.
- A blocker is always anchored to its required `PlanBlocker.phase`; its branch label names the actual classification and focuses that phase. Planning completion is not painted as an execution-stage failure.
- Repository eligibility is never summarized as generic unavailable. The interface distinguishes unreachable repository, unsupported project, blocked plan, missing runtime, limited run, and full verification. A uniquely traceable launch without a machine Oracle may end only as `started_unverified`.
- The evidence rail occupies 360-460 px, about one third of the working width, uses Geist Sans and Geist Mono, and presents progress, a prominent actual command, environment, and observations as continuous ruled sections. Normal controls are 32 px high; the principal session action is 36 px and never becomes a wide filled callout.
- The canvas never carries text, controls, progress truth, or the only representation of status. Reduced motion, hidden windows, missing WebGL, and context loss leave the DOM workflow fully usable.
- History and Settings do not use the elastic field.
