package dev.nimbus.core.render;

/**
 * Кривые анимаций.
 *
 * Равномерное движение глаз читает как дешёвое. Разница между дорогим и
 * дешёвым интерфейсом почти целиком в этих кривых.
 */
public final class Easing {

    private Easing() {
    }

    /** Быстрый старт, мягкая остановка. Основная кривая для появления элементов. */
    public static float outCubic(float t) {
        float x = clamp01(t);
        float inverted = 1f - x;
        return 1f - inverted * inverted * inverted;
    }

    /** Мягкий старт и мягкая остановка. Для переходов между состояниями. */
    public static float inOutCubic(float t) {
        float x = clamp01(t);
        if (x < 0.5f) {
            return 4f * x * x * x;
        }
        float shifted = -2f * x + 2f;
        return 1f - shifted * shifted * shifted / 2f;
    }

    /** Лёгкий перелёт за цель и возврат. Для появления окон и кнопок. */
    public static float outBack(float t) {
        float x = clamp01(t);
        float overshoot = 1.70158f;
        float shifted = x - 1f;
        return 1f + (overshoot + 1f) * shifted * shifted * shifted + overshoot * shifted * shifted;
    }

    /**
     * Плавное дотягивание текущего значения к целевому, не зависящее от частоты кадров.
     *
     * Простое сложение с коэффициентом даёт разную скорость на 60 и на 240 кадрах в
     * секунду, поэтому скорость задаётся временем полупути, а не долей за кадр.
     *
     * @param current    текущее значение
     * @param target     целевое значение
     * @param halfLife   за сколько секунд разрыв сокращается вдвое
     * @param deltaTime  сколько секунд прошло с прошлого кадра
     */
    public static float approach(float current, float target, float halfLife, float deltaTime) {
        if (halfLife <= 0f || deltaTime <= 0f) {
            return target;
        }
        double factor = Math.pow(0.5, deltaTime / halfLife);
        return (float) (target + (current - target) * factor);
    }

    private static float clamp01(float value) {
        if (value < 0f) {
            return 0f;
        }
        return value > 1f ? 1f : value;
    }
}
