package dev.nimbus.core.combat;

/**
 * Боевой трекер: кто был атакован последним и когда.
 *
 * Панель цели и звук попадания построены на опросе, а не на подписках: хук
 * атаки и отрисовка живут в одном игровом потоке, поэтому обычные поля здесь
 * безопасны, а блокировки не нужны вовсе.
 *
 * Цель хранится непрозрачным объектом: ядро не знает классов игры, данные
 * сущности достаются мостом версии по запросу.
 */
public final class CombatTracker {

    private Object target;
    private long lastHit;
    private int hits;

    /** Новая атака. Тот же объект - продолжение боя, другой - новая цель. */
    public void attack(Object entity) {
        if (entity == null) {
            return;
        }
        long now = System.nanoTime();
        if (entity == target) {
            hits++;
        } else {
            target = entity;
            hits = 1;
        }
        lastHit = now;
    }

    /** Текущая цель или null, если боя давно не было. */
    public Object target() {
        return target;
    }

    /** Нано-время последнего удара по текущей цели. */
    public long lastHitAt() {
        return lastHit;
    }

    /** Сколько ударов нанесено по текущей цели подряд. */
    public int hits() {
        return hits;
    }

    /** Свежа ли цель: после последнего удара прошло меньше удержания. */
    public boolean fresh(long holdMs) {
        return fade(holdMs) > 0f;
    }

    /**
     * Плавность показа панели: единица сразу после удара, ноль к концу удержания.
     *
     * Панель не исчезает скачком - она гаснет, и чем ближе конец удержания, тем
     * прозрачнее становится её отрисовка.
     */
    public float fade(long holdMs) {
        if (target == null) {
            return 0f;
        }
        long window = holdMs * 1_000_000L;
        long alive = System.nanoTime() - lastHit;
        if (alive >= window) {
            return 0f;
        }
        return 1f - alive / (float) window;
    }

    /** Забыть цель: при выходе из мира панель не должна показывать устаревшее. */
    public void reset() {
        target = null;
        hits = 0;
        lastHit = 0L;
    }
}
