package dev.nimbus.tools;

import dev.nimbus.core.Settings;
import dev.nimbus.core.config.Module;
import java.nio.file.Files;
import java.nio.file.Path;

/** Offline checks for Nimbus settings and ClickGui state persistence. */
public final class VerifySettingsPersistence {

    private VerifySettingsPersistence() {
    }

    public static void main(String[] args) throws Exception {
        Path root = Files.createTempDirectory("nimbus-settings-");
        try {
            Path file = root.resolve("client").resolve("settings.properties");
            int lastCategory = Module.Category.values().length - 1;
            Settings first = new Settings(file);
            first.module("fps").setOn(false);
            if (!first.module("fps").bool("low").get() || first.module("fps").bool("frameTime").get()) {
                throw new AssertionError("new FPS options have incorrect defaults");
            }
            if (first.module("memory").on()) {
                throw new AssertionError("memory module should be disabled by default");
            }
            first.module("theme").choice("surface").next();
            first.setGuiCategory(lastCategory);
            first.setGuiWindowPosition(0.82f, 0.17f);
            if (!first.dirty() || !first.saveIfDirty() || first.dirty() || first.saveFailed()) {
                throw new AssertionError("initial settings save failed");
            }

            Settings second = new Settings(file);
            second.load();
            if (second.module("fps").on()) {
                throw new AssertionError("module enabled state did not round-trip");
            }
            if (second.module("theme").choice("surface").index() != 1) {
                throw new AssertionError("option value did not round-trip");
            }
            expectClose(second.guiWindowX(), 0.82f, "gui.windowX");
            expectClose(second.guiWindowY(), 0.17f, "gui.windowY");
            if (second.guiCategory() != lastCategory) {
                throw new AssertionError("gui.category did not round-trip");
            }

            Path legacy = root.resolve("legacy.properties");
            Files.writeString(legacy, "fps.enabled=true\n");
            Settings legacySettings = new Settings(legacy);
            legacySettings.load();
            if (legacySettings.guiCategory() != 0
                    || legacySettings.guiWindowX() != 0.5f
                    || legacySettings.guiWindowY() != 0.5f) {
                throw new AssertionError("legacy defaults changed");
            }

            Path malformed = root.resolve("malformed.properties");
            Files.writeString(malformed,
                    "gui.category=999\n"
                            + "gui.windowX=not-a-number\n"
                            + "gui.windowY=4.5\n");
            Settings malformedSettings = new Settings(malformed);
            malformedSettings.load();
            if (malformedSettings.guiCategory() != lastCategory
                    || malformedSettings.guiWindowX() != 0.5f
                    || malformedSettings.guiWindowY() != 1f) {
                throw new AssertionError("malformed gui state was not normalized");
            }

            Path blockedParent = root.resolve("blocked");
            Files.writeString(blockedParent, "not a directory");
            Settings failed = new Settings(blockedParent.resolve("settings.properties"));
            failed.setGuiCategory(1);
            if (failed.saveIfDirty() || !failed.dirty() || !failed.saveFailed()) {
                throw new AssertionError("failed save was not retained");
            }
            Files.delete(blockedParent);
            Files.createDirectories(blockedParent);
            if (!failed.saveIfDirty() || failed.dirty() || failed.saveFailed()) {
                throw new AssertionError("failed save did not recover");
            }

            System.out.println("Verified Nimbus settings persistence");
        } finally {
            deleteTree(root);
        }
    }

    private static void expectClose(float actual, float expected, String key) {
        if (Math.abs(actual - expected) > 0.0005f) {
            throw new AssertionError(key + " expected " + expected + " but got " + actual);
        }
    }

    private static void deleteTree(Path path) throws Exception {
        if (!Files.exists(path)) {
            return;
        }
        try (var entries = Files.walk(path)) {
            entries.sorted((left, right) -> right.compareTo(left)).forEach(item -> {
                try {
                    Files.deleteIfExists(item);
                } catch (Exception error) {
                    throw new RuntimeException(error);
                }
            });
        }
    }
}
