import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { jarTool } from "./java-env.mjs";

const root = resolve(import.meta.dirname, "..");
const artifactDir = join(root, "src-tauri", "resources", "nimbus");
const jar = join(artifactDir, "nimbus-runtime.jar");
const manifestPath = join(artifactDir, "nimbus-runtime.json");

if (!existsSync(jar) || !existsSync(manifestPath)) {
  throw new Error("Nimbus runtime resources are missing; run npm run build:client first");
}

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const bytes = readFileSync(jar);
const hash = createHash("sha256").update(bytes).digest("hex");
if (manifest.schema !== 1 || manifest.clientVersion !== "0.1.0") {
  throw new Error("Nimbus runtime manifest has an unsupported schema or client version");
}
if (!manifest.minecraftVersions?.includes("1.20.1")) {
  throw new Error("Nimbus runtime manifest does not declare Minecraft 1.20.1 support");
}
if (manifest.size !== bytes.length || manifest.sha256.toLowerCase() !== hash) {
  throw new Error("Nimbus runtime manifest does not match the staged JAR");
}

const extractionDir = mkdtempSync(join(tmpdir(), "nimbus-artifact-"));
try {
  execFileSync(jarTool(), ["--extract", "--file", jar, "META-INF/MANIFEST.MF"], {
    cwd: extractionDir,
    stdio: "ignore",
  });
  const jarManifest = readFileSync(join(extractionDir, "META-INF", "MANIFEST.MF"), "utf8");
  const attributes = new Map();
  for (const line of jarManifest.split(/\r?\n/)) {
    const separator = line.indexOf(":");
    if (separator > 0) {
      attributes.set(line.slice(0, separator).trim().toLowerCase(), line.slice(separator + 1).trim());
    }
  }
  for (const [name, expected] of [
    ["Premain-Class", "dev.nimbus.runtime.NimbusAgent"],
    ["Agent-Class", "dev.nimbus.runtime.NimbusAgent"],
    ["Can-Retransform-Classes", "true"],
    ["Implementation-Version", "0.1.0"],
  ]) {
    if (attributes.get(name.toLowerCase()) !== expected) {
      throw new Error(`Nimbus runtime JAR has an invalid ${name}`);
    }
  }
} finally {
  rmSync(extractionDir, { recursive: true, force: true });
}

console.log(`Verified Nimbus runtime: ${bytes.length} bytes, SHA-256 ${hash}`);
