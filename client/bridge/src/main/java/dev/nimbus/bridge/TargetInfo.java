package dev.nimbus.bridge;

/**
 * Последняя атакованная сущность: то, что нужно панели цели.
 *
 * Сущность приходит из игры как непрозрачный объект и не выходит за пределы
 * моста: наружу отдаётся уже разобранная и безопасная форма.
 *
 * @param name      имя сущности, как его показывает игра
 * @param health    текущее здоровье
 * @param maxHealth полный запас здоровья
 * @param alive     жива ли сущность прямо сейчас
 * @param x         координата сущности по X
 * @param y         координата сущности по Y
 * @param z         координата сущности по Z
 */
public record TargetInfo(String name, float health, float maxHealth, boolean alive, double x, double y, double z) {

    /** Доля здоровья от 0 до 1; у сущности без здоровья - 0. */
    public float healthFraction() {
        if (maxHealth <= 0f) {
            return 0f;
        }
        return Math.max(0f, Math.min(1f, health / maxHealth));
    }
}
