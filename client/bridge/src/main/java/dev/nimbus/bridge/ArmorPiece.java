package dev.nimbus.bridge;

/**
 * Один предмет брони игрока с остатком прочности.
 *
 * @param name     название предмета; пустая строка, если слот пуст
 * @param durability сколько прочности снято
 * @param maxDurability полная прочность; ноль у предметов без прочности
 */
public record ArmorPiece(String name, int durability, int maxDurability) {

    /** Доля целой прочности от 0 до 1; у предметов без прочности - 1. */
    public float integrity() {
        if (maxDurability <= 0) {
            return 1f;
        }
        return Math.max(0f, Math.min(1f, 1f - durability / (float) maxDurability));
    }
}
