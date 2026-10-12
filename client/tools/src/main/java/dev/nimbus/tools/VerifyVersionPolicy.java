package dev.nimbus.tools;

import dev.nimbus.runtime.GameVersion;

/** Executable checks for the production Minecraft version gate. */
public final class VerifyVersionPolicy {

    private VerifyVersionPolicy() {
    }

    public static void main(String[] args) {
        expect(GameVersion.Support.SUPPORTED, GameVersion.classify("1.20.1"), "1.20.1");
        expect(GameVersion.Support.KNOWN_UNSUPPORTED, GameVersion.classify("1.20.2"), "1.20.2");
        expect(GameVersion.Support.KNOWN_UNSUPPORTED, GameVersion.classify("1.21.1"), "1.21.1");
        expect(GameVersion.Support.KNOWN_UNSUPPORTED, GameVersion.classify("1.20"), "1.20");
        expect(GameVersion.Support.UNKNOWN, GameVersion.classify("1.20.1-pre1"), "1.20.1-pre1");
        expect(GameVersion.Support.UNKNOWN, GameVersion.classify("snapshot"), "snapshot");
        expect(GameVersion.Support.UNKNOWN, GameVersion.classify("  "), "blank");
        expect(GameVersion.Support.UNKNOWN, GameVersion.classify(null), "null");

        if (!GameVersion.isSupported("1.20.1")) {
            throw new AssertionError("production version must be supported");
        }
        if (GameVersion.isSupported("1.21.1")) {
            throw new AssertionError("future versions must not activate the client");
        }

        System.out.println("Verified Nimbus Minecraft version policy");
    }

    private static void expect(GameVersion.Support expected, GameVersion.Support actual, String input) {
        if (actual != expected) {
            throw new AssertionError(
                    "version " + input + " expected " + expected + " but got " + actual
            );
        }
    }
}
