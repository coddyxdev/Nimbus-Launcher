package dev.nimbus.bridge;

/**
 * Единственная дверь из клиента в игру.
 *
 * Чем уже этот интерфейс, тем дешевле поддержка каждой новой версии Minecraft:
 * реализация живёт в adapters, вся логика — в core.
 */
public interface GameBridge {

    /** Версия запущенной игры, например 1.20.1. */
    String gameVersion();

    /** Ширина окна в пикселях. */
    int windowWidth();

    /** Высота окна в пикселях. */
    int windowHeight();

    /** Масштаб интерфейса игры. */
    double guiScale();

    /** Захвачен ли курсор игрой. */
    boolean cursorGrabbed();

    /** Отпустить или захватить курсор — нужно для меню по правому Shift. */
    void setCursorGrabbed(boolean grabbed);

    /** Сообщение в игровой чат (только локально, на сервер не уходит). */
    void printMessage(String text);
}
