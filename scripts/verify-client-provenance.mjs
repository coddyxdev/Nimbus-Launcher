import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { jarTool } from "./java-env.mjs";

const root = resolve(import.meta.dirname, "..");
const clientDir = join(root, "client");
const sourceExtensions = new Set([".java", ".kt", ".kts", ".gradle", ".properties"]);
const ignoredDirectories = new Set([".gradle", "build", ".nimbus-backup"]);
const forbiddenMarkers = [
  "kimiko",
  "com.kimiko",
  "geckolib",
  "waveycapes",
  "chatheads",
  "shulkerview",
  "chatanimation",
];
const forbiddenDependencyDeclarations = [
  /(?:^|\n)\s*(?:import|package)\s+com\.kimiko\b/im,
  /(?:^|\n)\s*(?:import|package)\s+(?:software\.bernie\.geckolib|dev\.tr7zw\.chatheads)\b/im,
];
const forbiddenJarPrefixes = [
  "net/minecraft/",
  "net/fabricmc/",
  "org/quiltmc/",
  "net/minecraftforge/",
  "com/mojang/blaze3d/",
];
const forbiddenJarMarkers = [
  "kimiko",
  "geckolib",
  "waveycapes",
  "chatheads",
  "shulkerview",
  "chatanimation",
];

const violations = [];

function walk(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!ignoredDirectories.has(entry.name)) {
        walk(join(directory, entry.name));
      }
      continue;
    }
    const path = join(directory, entry.name);
    if (!sourceExtensions.has(path.slice(path.lastIndexOf(".")))) {
      continue;
    }
    const source = readFileSync(path, "utf8");
    const lower = source.toLowerCase();
    for (const marker of forbiddenMarkers) {
      if (lower.includes(marker)) {
        violations.push(`${relative(root, path)} contains forbidden marker ${JSON.stringify(marker)}`);
      }
    }
    for (const pattern of forbiddenDependencyDeclarations) {
      if (pattern.test(source)) {
        violations.push(`${relative(root, path)} contains a forbidden dependency declaration`);
      }
    }
  }
}

if (!statSync(clientDir, { throwIfNoEntry: false })) {
  throw new Error(`Nimbus client source directory is missing: ${clientDir}`);
}
walk(clientDir);

const jarPath = join(root, "src-tauri", "resources", "nimbus", "nimbus-runtime.jar");
if (statSync(jarPath, { throwIfNoEntry: false })) {
  execFileSync(jarTool(), ["--list", "--file", jarPath], { encoding: "utf8" })
    .split(/\r?\n/)
    .filter(Boolean)
    .forEach((entry) => {
      const lower = entry.toLowerCase();
      if (forbiddenJarPrefixes.some((prefix) => lower.startsWith(prefix))
        || forbiddenJarMarkers.some((marker) => lower.includes(marker))) {
        violations.push(`src-tauri/resources/nimbus/nimbus-runtime.jar contains forbidden entry ${entry}`);
      }
    });
}

if (violations.length > 0) {
  throw new Error(`Nimbus client provenance check failed:\n- ${violations.join("\n- ")}`);
}

console.log("Verified Nimbus client provenance markers");
