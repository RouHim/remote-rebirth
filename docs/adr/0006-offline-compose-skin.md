# Compose declares profile and skin

The model declares `profile` and `skin`, so `compose` runs without a connected remote and produces deterministic output. Scenario 2's connected remote is used by `write`, not by `compose`. `write` rejects a container whose profile or skin differs from the connected remote.

## Consequences

The model is not portable across skins without editing. Favourite capacity follows the declared skin.
