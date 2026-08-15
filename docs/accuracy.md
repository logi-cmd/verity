# Accuracy contract

`verified` is issued only when all planned phases and the target-specific machine oracle succeed on the exact recorded snapshot. A process, port, generated artifact, or window signal alone yields `started_unverified` at most.

Every automatically selected command records a manifest path, key, precedence, and working directory. Plan, environment, and oracle status are independent. A missing repository lock is a repository blocker; a missing tool in Verity's generated environment is a `verity_plan` blocker; an unavailable host toolchain is a runtime blocker. Oracle absence never masquerades as a repository command failure.

Failure output is described as the first observed blocker. Verity does not claim that it found the only root cause or that another blocker cannot exist later in the lifecycle.

Cleanup conclusions are narrower than general dead-code claims. `removal_verified` means that deleting the recorded files from the recorded snapshot still passed the same build, test, launch, and machine oracle. It does not prove safety for unknown external consumers or production paths outside that oracle. Analyzer-only results, protected surfaces, and all results from `started_unverified` sessions remain report-only.
