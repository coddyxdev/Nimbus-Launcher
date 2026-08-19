package dev.nimbus.bridge;

/**
 * Копилка прокрутки колеса мыши.
 *
 * Колесо - единственный вид ввода, который нельзя опросить у оконной библиотеки:
 * у GLFW нет функции "сколько накрутили", есть только событие. Событие ловит рантайм
 * в обработчике игры и кладёт сюда, а ядро забирает накопленное в своём кадре.
 *
 * Класс живёт в мосту, потому что его видят обе стороны: и рантайм, и адаптер.
 * Значение копится, а не замещается: за один кадр событий может прийти несколько,
 * и терять их значит терять прокрутку при быстром вращении.
 */
public final class ScrollBuffer {

    /**
     * Ограничитель накопления.
     *
     * Если ядро почему-то перестало забирать накопленное, прокрутка не должна
     * копиться бесконечно и выстреливать одним рывком через минуту.
     */
    private static final double LIMIT = 20.0;

    private static volatile double pending;

    private ScrollBuffer() {
    }

    /** Записать событие колеса. Вызывается из игрового потока. */
    public static void push(double amount) {
        if (amount == 0.0 || Double.isNaN(amount)) {
            return;
        }
        double next = pending + amount;
        if (next > LIMIT) {
            next = LIMIT;
        } else if (next < -LIMIT) {
            next = -LIMIT;
        }
        pending = next;
    }

    /** Забрать накопленное и обнулить копилку. */
    public static double take() {
        double value = pending;
        if (value != 0.0) {
            pending = 0.0;
        }
        return value;
    }

    /** Забыть накопленное: нужно при закрытии меню, чтобы не дёргать его при следующем открытии. */
    public static void clear() {
        pending = 0.0;
    }
}
