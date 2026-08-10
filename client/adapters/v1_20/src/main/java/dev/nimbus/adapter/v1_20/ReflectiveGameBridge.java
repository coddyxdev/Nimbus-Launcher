package dev.nimbus.adapter.v1_20;

import dev.nimbus.bridge.GameBridge;

/**
 * Адаптер для ветки 1.20.x и выше.
 *
 * Заглушка этапа 1: интерфейс уже есть, реальные вызовы через MethodHandle
 * появятся вместе со слоем рендера на этапе 2.
 */
public final class ReflectiveGameBridge implements GameBridge {

    private final String gameVersion;

    public ReflectiveGameBridge(String gameVersion) {
        this.gameVersion = gameVersion;
    }

    @Override
    public String gameVersion() {
        return gameVersion;
    }

    @Override
    public int windowWidth() {
        return 0;
    }

    @Override
    public int windowHeight() {
        return 0;
    }

    @Override
    public double guiScale() {
        return 1.0;
    }

    @Override
    public boolean cursorGrabbed() {
        return true;
    }

    @Override
    public void setCursorGrabbed(boolean grabbed) {
        // Этап 2.
    }

    @Override
    public void printMessage(String text) {
        System.out.println("[Nimbus] " + text);
    }
}
