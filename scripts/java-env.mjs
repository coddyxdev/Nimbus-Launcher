import { existsSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

/**
 * Поиск Java для клиентских скриптов.
 *
 * Порядок: переменная JAVA_HOME, затем JDK, лежащий в комплекте лаунчера
 * (.tools/jdk17/jdk-*). Ничего настраивать в системе не нужно: репозиторий
 * самодостаточен. Возвращает окружение для дочерних процессов и путь к jar.
 */
const root = resolve(import.meta.dirname, "..");

function hasJava(home) {
  if (!home) {
    return false;
  }
  const tool = process.platform === "win32" ? "java.exe" : "java";
  return existsSync(join(home, "bin", tool));
}

function bundledHome() {
  const base = join(root, ".tools", "jdk17");
  if (!existsSync(base)) {
    return null;
  }
  for (const entry of readdirSync(base)) {
    const home = join(base, entry);
    if (hasJava(home)) {
      return home;
    }
  }
  return null;
}

function findHome() {
  if (hasJava(process.env.JAVA_HOME)) {
    return process.env.JAVA_HOME;
  }
  return bundledHome();
}

export function javaEnv() {
  const home = findHome();
  if (!home) {
    return null;
  }
  const bin = join(home, "bin");
  const pathKey = process.platform === "win32" ? "Path" : "PATH";
  const currentPath = process.env[pathKey] ?? "";
  return {
    JAVA_HOME: home,
    [pathKey]: `${bin}${process.platform === "win32" ? ";" : ":"}${currentPath}`,
  };
}

/** Полный путь к jar-утилите JDK: надёжнее поиска по PATH. */
export function jarTool() {
  const home = findHome();
  const tool = process.platform === "win32" ? "jar.exe" : "jar";
  if (home && existsSync(join(home, "bin", tool))) {
    return join(home, "bin", tool);
  }
  return tool;
}
