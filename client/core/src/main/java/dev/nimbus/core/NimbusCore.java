package dev.nimbus.core;

import dev.nimbus.bridge.GameBridge;

/**
 * Ядро клиента: вся логика, которая не зависит от версии Minecraft.
 *
 * Сюда пойдут UI, модули, конфиг, профили, локализация.
 * Правило: если код можно написать без обращения к игре — он пишется здесь.
 */
public final class NimbusCore {

    private static NimbusCore instance;

    private final GameBridge game;
    private boolean menuOpen;

    private NimbusCore(GameBridge game) {
        this.game = game;
    }

    public static synchronized NimbusCore start(GameBridge game) {
        if (instance == null) {
            instance = new NimbusCore(game);
        }
        return instance;
    }

    public static NimbusCore instance() {
        return instance;
    }

    public GameBridge game() {
        return game;
    }

    public boolean menuOpen() {
        return menuOpen;
    }

    /** Открыть или закрыть меню по правому Shift. Паузу игре не ставим. */
    public void toggleMenu() {
        menuOpen = !menuOpen;
        game.setCursorGrabbed(!menuOpen);
    }

    /** Игровой тик. Вызывается из игрового потока. */
    public void tick() {
        // Модули появятся на этапе 4.
    }
}
