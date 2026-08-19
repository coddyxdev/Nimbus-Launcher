package dev.nimbus.core;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Keys;
import dev.nimbus.bridge.ScrollBuffer;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.hud.Hud;
import dev.nimbus.core.ui.ClickGui;
import dev.nimbus.core.ui.Radial;

/**
 * Ядро клиента: вся логика, которая не зависит от версии Minecraft.
 *
 * Два способа управления сознательно разнесены: полное окно открывается нажатием
 * и живёт, пока его не закроют, а колесо живёт только пока клавиша зажата.
 * Одновременно они не открываются никогда.
 *
 * Ввод. Клавиши опрашиваются напрямую у оконной библиотеки, а не через подмену
 * обработчиков игры: меньше патчей байт-кода - меньше поводов сломать чужую логику.
 * Опрос идёт и в тике, и в кадре: тики идут двадцать раз в секунду, и меню, открытое
 * только по тикам, ощущается тормозным даже на хорошем компьютере.
 *
 * Колесо мыши - единственный ввод, который опросить нельзя: у оконной библиотеки
 * есть только событие. Его ловит рантайм и кладёт в общую копилку, а меню забирает
 * накопленное в своём кадре.
 *
 * Курсор. Пока ядро открыто, курсор должен быть отпущен: игра поворачивает
 * камеру только при захваченном курсоре. При этом сама игра забирает курсор обратно
 * при любом клике в окно, а повторный захват швыряет его в центр экрана. Поэтому
 * правильное место лечения - не здесь, а в самом обработчике нажатия: пока меню открыто,
 * игра кликов не видит вовсе. Здешнее переотпускание курсора остаётся запасным
 * вариантом для версий, где перехват не встал.
 *
 * Захват возвращается только если мы его сами и сняли: иначе мы бы захватывали
 * курсор поверх открытого инвентаря или чата.
 */
public final class NimbusCore {

    private static NimbusCore instance;

    private final GameBridge game;
    private final Settings settings = new Settings();
    private final Hud hud;
    private final Radial radial;
    private final ClickGui gui;

    private boolean menuWasDown;
    private boolean wheelWasDown;
    private boolean escapeWasDown;
    private boolean releasedByUs;
    private boolean wasOpen;

    /** Правда, если перехват кликов действительно вставлен в игру. */
    private boolean gameMouseBlocked;

    private NimbusCore(GameBridge game) {
        this.game = game;
        settings.load();
        this.hud = new Hud(settings);
        this.radial = new Radial(settings);
        this.gui = new ClickGui(settings);
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

    public Settings settings() {
        return settings;
    }

    public boolean menuOpen() {
        return gui.visible() || radial.visible() || hud.editing();
    }

    /**
     * Прятать ли ванильный прицел.
     *
     * Условие обязано сходиться с тем, по которому HUD рисует свой прицел, иначе
     * получается либо два прицела сразу, либо ни одного. Раньше сходилось не всё:
     * HUD молчит, пока открыто меню, а ванильный прицел всё равно прятался - и в меню
     * не оставалось никакого прицела вообще.
     *
     * Пока открыт любой наш экран, прицел не нужен вовсе: ровно так же игра
     * поступает со своими экранами.
     */
    public boolean hideVanillaCrosshair() {
        try {
            if (menuOpen()) {
                return true;
            }
            Module crosshair = settings.module("crosshair");
            return crosshair != null && crosshair.on() && game.inWorld();
        } catch (Throwable error) {
            return false;
        }
    }

    public void setGameMouseBlocked(boolean blocked) {
        this.gameMouseBlocked = blocked;
    }

    /** Игровой тик. Вызывается из игрового потока. */
    public void tick() {
        if (!game.ready()) {
            return;
        }
        pollInput();
    }

    /**
     * Отрисовка поверх игрового интерфейса. Вызывается каждый кадр.
     *
     * Контекст рисования действителен только внутри этого вызова, поэтому он
     * обязательно отпускается в finally: удержанная ссылка на объект игры - утечка.
     */
    public void renderHud(Object graphics, float partialTick) {
        game.beginFrame(graphics, partialTick);
        try {
            if (!game.ready()) {
                return;
            }
            pollInput();

            boolean open = menuOpen();
            if (!gameMouseBlocked && open && game.cursorGrabbed()) {
                game.setCursorGrabbed(false);
                releasedByUs = true;
            }

            hud.render(game, game.gameVersion(), open);
            radial.render(game);
            gui.render(game, game.gameVersion(), hud.fps());
        } finally {
            game.endFrame();
        }
    }

    /**
     * Общий опрос ввода.
     *
     * Вызывается и из тика, и из кадра, поэтому всё построено на фронтах
     * "было отжато - стало нажато": повторный вызов в том же состоянии ничего не делает.
     */
    private void pollInput() {
        boolean menuDown = game.keyDown(settings.menuKey());
        if (menuDown && !menuWasDown && !radial.visible()) {
            if (hud.editing()) {
                hud.stopEditing();
            } else {
                gui.toggle();
            }
        }
        menuWasDown = menuDown;

        boolean wheelDown = game.keyDown(settings.wheelKey());
        if (wheelDown && !wheelWasDown && !gui.visible() && !hud.editing()) {
            radial.beginGesture();
        } else if (!wheelDown && wheelWasDown) {
            radial.endGesture();
        }
        wheelWasDown = wheelDown;

        boolean escapeDown = game.keyDown(Keys.ESCAPE);
        if (escapeDown && !escapeWasDown) {
            if (hud.editing()) {
                hud.stopEditing();
            } else if (gui.visible()) {
                gui.close();
            } else if (radial.visible()) {
                radial.close();
            }
        }
        escapeWasDown = escapeDown;

        // Кнопка "Расставить панели" в меню только просит режим: само меню не знает про HUD.
        if (gui.takeLayoutRequest()) {
            gui.close();
            hud.startEditing();
        }

        boolean open = menuOpen();
        if (open) {
            if (game.cursorGrabbed()) {
                game.setCursorGrabbed(false);
                releasedByUs = true;
            }
        } else if (wasOpen) {
            if (releasedByUs) {
                game.setCursorGrabbed(true);
            }
            releasedByUs = false;
            // Накопленное колесо забывается, иначе при следующем открытии список дёрнется.
            ScrollBuffer.clear();
            settings.saveIfDirty();
        }
        wasOpen = open;
    }
}
