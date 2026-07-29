import {
  accessSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
} from "node:fs";
import { constants as fsConstants } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const assetsRoot = join(repositoryRoot, "frontend", "assets");
const manifestPath = join(assetsRoot, "aseprite-assets.json");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const command = process.argv[2] ?? "build";
const pollIntervalMs = Number(process.env.ASSET_WATCH_INTERVAL_MS ?? 350);

function exists(path) {
  try {
    accessSync(path, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

function assetPaths(asset) {
  return {
    source: join(assetsRoot, asset.source),
    output: join(assetsRoot, asset.output),
  };
}

function findAseprite() {
  const candidates = [
    process.env.ASEPRITE_BIN,
    process.platform === "darwin"
      ? "/Applications/Aseprite.app/Contents/MacOS/aseprite"
      : undefined,
    "aseprite",
  ].filter(Boolean);

  for (const candidate of candidates) {
    const result = spawnSync(candidate, ["--version"], { stdio: "ignore" });
    if (!result.error && result.status === 0) {
      return candidate;
    }
  }

  throw new Error(
    "Aseprite was not found. Install it or set ASEPRITE_BIN to its executable.",
  );
}

function runAseprite(executable, args) {
  const result = spawnSync(executable, args, { stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`Aseprite exited with status ${result.status}.`);
  }
}

function exportAsset(executable, asset) {
  const { source, output } = assetPaths(asset);
  if (!exists(source)) {
    throw new Error(`Missing source: ${asset.source}`);
  }

  mkdirSync(dirname(output), { recursive: true });
  const temporaryOutput = `${output}.tmp.png`;
  try {
    runAseprite(executable, [
      "--batch",
      source,
      "--sheet-type",
      "horizontal",
      "--sheet",
      temporaryOutput,
    ]);
    renameSync(temporaryOutput, output);
  } finally {
    if (exists(temporaryOutput)) {
      unlinkSync(temporaryOutput);
    }
  }
  console.log(`exported ${asset.source} -> ${asset.output}`);
}

function bootstrapAsset(executable, asset) {
  const { source, output } = assetPaths(asset);
  if (exists(source)) {
    console.log(`kept     ${asset.source}`);
    return;
  }
  if (!exists(output)) {
    throw new Error(`Missing generated asset: ${asset.output}`);
  }

  mkdirSync(dirname(source), { recursive: true });
  const temporarySource = `${source}.tmp.aseprite`;
  try {
    copyFileSync(output, `${output}.bootstrap.png`);
    runAseprite(executable, [
      "--batch",
      `${output}.bootstrap.png`,
      "--save-as",
      temporarySource,
    ]);
    renameSync(temporarySource, source);
  } finally {
    if (exists(temporarySource)) {
      unlinkSync(temporarySource);
    }
    if (exists(`${output}.bootstrap.png`)) {
      unlinkSync(`${output}.bootstrap.png`);
    }
  }
  console.log(`created  ${asset.source}`);
}

function signature(path) {
  if (!exists(path)) {
    return null;
  }
  const stat = statSync(path);
  return `${stat.mtimeMs}:${stat.size}`;
}

function buildAll() {
  const executable = findAseprite();
  for (const asset of manifest.assets) {
    exportAsset(executable, asset);
  }
}

function bootstrapAll() {
  const executable = findAseprite();
  for (const asset of manifest.assets) {
    bootstrapAsset(executable, asset);
  }
  console.log("Aseprite sources are ready. Run `make assets-build` to verify them.");
}

function watch() {
  const executable = findAseprite();
  const signatures = new Map();
  let warnedAboutMissingSource = false;

  for (const asset of manifest.assets) {
    const { source } = assetPaths(asset);
    signatures.set(asset.source, signature(source));
    if (exists(source)) {
      exportAsset(executable, asset);
    } else {
      warnedAboutMissingSource = true;
      console.warn(`waiting for ${asset.source}`);
    }
  }

  if (warnedAboutMissingSource) {
    console.warn("Run `make assets-bootstrap` once to create missing sources.");
  }
  console.log(`watching Aseprite assets every ${pollIntervalMs}ms`);

  setInterval(() => {
    for (const asset of manifest.assets) {
      const { source } = assetPaths(asset);
      const nextSignature = signature(source);
      if (nextSignature === signatures.get(asset.source)) {
        continue;
      }
      signatures.set(asset.source, nextSignature);
      if (nextSignature === null) {
        console.warn(`source removed: ${asset.source}`);
        continue;
      }
      try {
        exportAsset(executable, asset);
      } catch (error) {
        console.error(error.message);
      }
    }
  }, pollIntervalMs);
}

try {
  if (!Array.isArray(manifest.assets) || manifest.assets.length === 0) {
    throw new Error("aseprite-assets.json must contain at least one asset.");
  }

  switch (command) {
    case "build":
      buildAll();
      break;
    case "bootstrap":
      bootstrapAll();
      break;
    case "watch":
      watch();
      break;
    default:
      throw new Error("Usage: aseprite_assets.mjs <build|bootstrap|watch>");
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
