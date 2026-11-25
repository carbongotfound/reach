// node tests/app/model-check.mjs — bundles model-check.ts (model.tsx has JSX) with the app's own bundler and runs it.
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(join(here, "../../app/package.json"));
const { rolldown } = await import(pathToFileURL(require.resolve("rolldown")).href);
const out = mkdtempSync(join(tmpdir(), "reach-model-check-"));
try {
  const b = await rolldown({ input: join(here, "model-check.ts"), platform: "node", external: [/^node:/], transform: { jsx: "react-jsx" }, logLevel: "silent" });
  await b.write({ file: join(out, "check.mjs"), format: "esm" });
  await import(pathToFileURL(join(out, "check.mjs")).href);
} finally { rmSync(out, { recursive: true, force: true }); }
