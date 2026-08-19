package dev.nimbus.runtime;

import dev.nimbus.adapter.v1_20.ReflectiveGameBridge;
import dev.nimbus.core.NimbusCore;

/**
 * Точки входа, которые вызывает вставленный в игру байт-код.
 *
 * Методы вызываются из игрового потока десятки раз в секунду:
 * никаких аллокаций и никаких исключений наружу - упавший хук уронит игру.
 */
public final class NimbusHooks {

    /**
     * Через столько тиков (около пяти секунд) в лог уходит единственная
     * строка о том, что игровой цикл действительно наш. Без неё убедиться в
     * этом можно только в отладочном режиме, а он выключен у игроков.
     */
    private static final long CONFIRM_AFTER_TICKS = 100L;

    /** То же самое для отрисовки: один раз подтверждаем, что кадр наш. */
    private static final long CONFIRM_AFTER_FRAMES = 100L;

    private static volatile boolean started;
    private static long ticks;
    private static long frames;
    private static long lastReport = System.nanoTime();

    private static NimbusCore core;
    private static ReflectiveGameBridge bridge;
    private static boolean coreFailed;
    private static boolean bridgeFailureLogged;
    private static volatile boolean mousePatched;

    private NimbusHooks() {
    }

    /** Вызывается один раз при создании клиента. */
    public static void onGameStart() {
        if (started) {
            return;
        }
        started = true;
        try {
            Log.info("игра запущена, клиент внутри");
        } catch (Throwable error) {
            safeReport(error);
        }
    }

    /** Вызывается каждый игровой тик. */
    public static void onTick() {
        try {
            ticks++;
            if (ticks == CONFIRM_AFTER_TICKS) {
                Log.info("игровой цикл подключён, тиков: " + ticks);
            }
            NimbusCore instance = core;
            if (instance != null) {
                instance.tick();
            }
            if (!Log.debugEnabled()) {
                return;
            }
            long now = System.nanoTime();
            if (now - lastReport >= 5_000_000_000L) {
                lastReport = now;
                Log.debug("тиков всего: " + ticks);
            }
        } catch (Throwable error) {
            safeReport(error);
        }
    }

    /**
     * Вызывается в конце отрисовки игрового интерфейса, то есть поверх всего,
     * что игра уже нарисовала в этом кадре.
     *
     * @param graphics    контекст рисования игры (GuiGraphics). Здесь он
     *                    приходит как Object: рантайм не знает классов игры,
     *                    их имена в каждой версии свои. Разбирать его будет
     *                    мост версии, а не этот класс.
     * @param partialTick доля тика между обновлениями логики: нужна, чтобы
     *                    анимации шли плавно, а не ступеньками по 20 раз в секунду.
     */
    public static void onRenderHud(Object graphics, float partialTick) {
        try {
            frames++;
            if (frames == CONFIRM_AFTER_FRAMES) {
                Log.info("слой отрисовки подключён, кадров: " + frames
                        + ", контекст: " + (graphics == null ? "нет" : graphics.getClass().getName()));
            }
            NimbusCore instance = core();
            if (instance == null) {
                return;
            }
            instance.renderHud(graphics, partialTick);
            reportBridgeFailureOnce();
        } catch (Throwable error) {
            safeReport(error);
        }
    }

    /**
     * Забрать ли у игры обработку кнопки мыши.
     *
     * Вызывается из самого начала обработчика нажатия. Если вернёт истину, игра
     * не увидит клика вовсе: не ударит по блоку, не поставит блок и, главное, не
     * захватит курсор обратно. Именно обратный захват швырял курсор в центр экрана
     * при каждом клике по меню.
     *
     * Свои клики мы читаем напрямую у оконной библиотеки, поэтому перехват нашему
     * собственному интерфейсу не мешает.
     */
    /**
     * Событие колеса мыши.
     *
     * Пока открыто меню, прокрутка уходит нам и не доходит до игры: иначе
     * листание списка одновременно меняет быстрый слот или вкладку инвентаря.
     *
     * Возвращает true, если событие съедено.
     */
    public static boolean onScroll(double delta) {
        try {
            NimbusCore instance = core;
            if (instance == null || !instance.menuOpen()) {
                return false;
            }
            dev.nimbus.bridge.ScrollBuffer.push(delta);
            return true;
        } catch (Throwable error) {
            // Из хука никогда не должно вылетать исключение: это убьёт ввод игры.
            safeReport(error);
            return false;
        }
    }

    /**
     * Прятать ли ванильный прицел.
     *
     * Нужно, когда включён свой прицел: иначе два прицела накладываются друг на друга.
     */
    public static boolean blockCrosshair() {
        try {
            NimbusCore instance = core;
            return instance != null && instance.hideVanillaCrosshair();
        } catch (Throwable error) {
            safeReport(error);
            return false;
        }
    }

    public static boolean blockMouse() {
        try {
            NimbusCore instance = core;
            return instance != null && instance.menuOpen();
        } catch (Throwable error) {
            return false;
        }
    }

    /** Трансформер сообщает, что перехват мыши действительно вставлен в игру. */
    public static void markMousePatched() {
        mousePatched = true;
        NimbusCore instance = core;
        if (instance != null) {
            instance.setGameMouseBlocked(true);
        }
    }

    public static long ticks() {
        return ticks;
    }

    public static long frames() {
        return frames;
    }

    /**
     * Ядро собирается при первом кадре, а не при старте агента.
     *
     * Раньше нельзя: мосту нужен живой контекст рисования, чтобы взять у него
     * загрузчик классов игры - системный загрузчик про классы игры не знает.
     */
    private static NimbusCore core() {
        if (core != null || coreFailed) {
            return core;
        }
        try {
            bridge = new ReflectiveGameBridge(
                    NimbusAgent.gameVersion(),
                    new MappingsBridge(NimbusAgent.mappings())
            );
            core = NimbusCore.start(bridge);
            core.setGameMouseBlocked(mousePatched);
            return core;
        } catch (Throwable error) {
            coreFailed = true;
            Log.error("ядро клиента не запустилось, игра продолжит без интерфейса", error);
            return null;
        }
    }

    /**
     * Мост никогда не бросает исключений, поэтому о его отказе можно узнать
     * только спросив его напрямую. Одна строка в лог вместо сотни в секунду.
     */
    private static void reportBridgeFailureOnce() {
        if (bridgeFailureLogged || bridge == null || bridge.ready()) {
            return;
        }
        String failure = bridge.failure();
        if (failure == null) {
            return;
        }
        bridgeFailureLogged = true;
        Log.warn("слой отрисовки отключён: " + failure);
    }

    private static void safeReport(Throwable error) {
        try {
            Log.error("хук упал", error);
        } catch (Throwable ignored) {
            // Даже логгер не должен ломать игру.
        }
    }
}
