import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, copyFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { javaEnv } from "./java-env.mjs";

const root = resolve(import.meta.dirname, "..");
const clientDir = join(root, "client");
const wrapperPath = join(clientDir, process.platform === "win32" ? "gradlew.bat" : "gradlew");
const wrapper = process.platform === "win32" ? `"${wrapperPath}"` : wrapperPath;
const jar = join(clientDir, "runtime", "build", "libs", "nimbus-runtime-0.1.0.jar");
const outputDir = join(root, "src-tauri", "resources", "nimbus");
const outputJar = join(outputDir, "nimbus-runtime.jar");
const outputManifest = join(outputDir, "nimbus-runtime.json");
const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

mkdirSync(outputDir, { recursive: true });
const result = spawnSync(wrapper, [":runtime:jar", "--no-daemon"], {
  cwd: clientDir,
  stdio: "inherit",
  shell: process.platform === "win32",
  // JDK из комплекта или JAVA_HOME уходит ребёнку-gradle как есть.
  env: { ...process.env, ...(javaEnv() ?? {}) },
});
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
if (!existsSync(jar)) {
  console.error(`Nimbus runtime jar was not produced: ${jar}`);
  process.exit(1);
}

copyFileSync(jar, outputJar);
const bytes = readFileSync(outputJar);
const sha256 = createHash("sha256").update(bytes).digest("hex");
writeFileSync(
  outputManifest,
  `${JSON.stringify(
    {
      schema: 1,
      clientVersion: "0.1.0",
      minecraftVersions: ["1.20.1"],
      launcherVersion: packageJson.version,
      sha256,
      size: bytes.length,
    },
    null,
    2,
  )}\n`,
);
console.log(`Staged Nimbus runtime (${bytes.length} bytes) in ${outputDir}`);
