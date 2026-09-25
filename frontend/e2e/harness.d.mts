// The types for `harness.mjs`, which cannot be TypeScript.
//
// It is `.mjs` because both the compiled Playwright config and the harness's
// plain-Node static server import it.
import type { HarnessSpec } from '@xinutec/ui-harness/config';

declare const spec: HarnessSpec;
export default spec;
