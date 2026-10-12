package dev.nimbus.core;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Keys;
import dev.nimbus.bridge.ScrollBuffer;
import dev.nimbus.bridge.Sounds;
import dev.nimbus.core.combat.CombatTracker;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.config.Option;
import dev.nimbus.core.hud.Hud;
import dev.nimbus.core.ui.ClickGui;
import dev.nimbus.core.ui.NotificationCenter;
import dev.nimbus.core.ui.Radial;

import java.util.HashMap;
import java.util.Map;

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
 *
 * Граница возможностей. Ядро только читает состояние игры и рисует поверх неё.
 * Оно не отправляет серверу ничего от имени игрока, не подделывает нажатия и не
 * трогает настройки за пределами их же диапазонов. Отсюда нет и не должно появиться
 * пути к чату, командам, пакетам, инвентарю или гамме выше ванильной: любая такая
 * функция превращает оверлей в чит, а клиент - в повод для бана.
 */
public final class NimbusCore {

    private static NimbusCore instance;

    private final GameBridge game;
    private final Settings settings = new Settings();
    private final Hud hud;
    private final Radial radial;
    private final ClickGui gui;
    private final NotificationCenter notifications = new NotificationCenter();
    private final CombatTracker combat = new CombatTracker();

    private boolean menuWasDown;
    private boolean wheelWasDown;
    private boolean escapeWasDown;
    private boolean releasedByUs;
    private boolean wasOpen;

    /** Правда, если перехват кликов действительно вставлен в игру. */
    private boolean gameMouseBlocked;

    /** Состояние "клавиша бинда была зажата" по каждому модулю. */
    private final Map<String, Boolean> bindWas = new HashMap<>();

    /** Плавный уровень зума: ноль - камера как обычно, единица - полный зум. */
    private float zoomLevel;

    /** Была ли плавная камера включена нами и подлежит ли возврату. */
    private boolean smoothCameraOurs;

    private NimbusCore(GameBridge game) {
        this.game = game;
        settings.load();
        this.hud = new Hud(settings);
        this.radial = new Radial(settings);
        this.gui = new ClickGui(settings);
        notifications.push("Nimbus готов", NotificationCenter.Level.SUCCESS, 2_000_000_000L);
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

    public NotificationCenter notifications() {
        return notifications;
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
            notifications.render(game, settings.accent());
        } finally {
            game.endFrame();
        }
    }

    /** Боевой трекер: цель последнего удара и её возраст. */
    public CombatTracker combat() {
        return combat;
    }

    /**
     * Событие атаки из боевого хука.
     *
     * Приходит на игровом потоке из начала attack: цель - непрозрачный объект игры.
     * Заодно бьёт в колокол модуля звука попадания: удар был, значит и звук был.
     */
    public void onCombatAttack(Object entity) {
        combat.attack(entity);
        try {
            Module hitsound = settings.module("hitsound");
            if (hitsound != null && hitsound.on()) {
                float base = 0.4f + hitsound.slider("pitch").get() * 0.6f;
                float pitch = base;
                if (hitsound.bool("vary").get()) {
                    pitch += (float) (Math.random() * 0.12 - 0.06);
                }
                game.playSound(hitsound.choice("sound").index() == 0 ? Sounds.HIT : Sounds.HIT_CRIT, pitch);
            }
        } catch (Throwable ignored) {
            // Звук - украшение: не сыгрался, так не сыгрался.
        }
    }

    /**
     * Модификатор угла обзора для зума.
     *
     * Зовётся из мира отрисовки много раз за кадр, поэтому при выключенном зуме
     * путь короткий: проверка уровня и возврат. Сама анимация живёт здесь же -
     * это единственное место, которое вызывается каждый кадр в мире.
     *
     * Угол приходит и уходит числом двойной точности: у Mojang этот метод отдаёт
     * float в старых версиях и double в новых, и сужать значение здесь значит
     * терять точность на всех новых.
     */
    public double zoomFov(double fov) {
        Module zoom = settings.moduleOrNull("zoom");
        if (zoom == null || !zoom.on()) {
            if (zoomLevel != 0f || smoothCameraOurs) {
                zoomLevel = 0f;
                if (smoothCameraOurs) {
                    smoothCameraOurs = false;
                    game.setSmoothCamera(false);
                }
            }
            return fov;
        }
        int key = zoom.keyOption("key").code();
        boolean want = key != Keys.NONE && game.keyDown(key) && game.inWorld();
        float target = want ? 1f : 0f;
        float speed = want ? 0.14f : 0.26f;
        zoomLevel += (target - zoomLevel) * speed;
        if (Math.abs(target - zoomLevel) < 0.002f) {
            zoomLevel = target;
        }
        boolean smooth = zoomLevel > 0.35f;
        if (smooth != smoothCameraOurs) {
            smoothCameraOurs = smooth;
            game.setSmoothCamera(smooth);
        }
        float power = zoom.slider("power").get();
        return fov * (1.0 - power * zoomLevel * zoomLevel);
    }

    /**
     * Клавиша игры, съеденная нами.
     *
     * Пока поле в меню ловит текст, каждая клавиша уходит в это поле и не доходит
     * до игры. Возвращает true, если событие обработано и игре оно не нужно.
     */
    public boolean onGameKey(int key, int action) {
        return gui.textKey(key, action);
    }

    /**
     * Спрятать ли часть данных в HUD-плашках (режим стримера).
     *
     * @param kind что просят спрятать: "coords", "server" или "name"
     */
    public boolean privacy(String kind) {
        try {
            Module streamer = settings.module("streamer");
            return streamer != null && streamer.on() && streamer.bool(kind).get();
        } catch (Throwable error) {
            return false;
        }
    }

    /**
     * Общий опрос ввода.
     *
     * Вызывается и из тика, и из кадра, поэтому всё построено на фронтах
     * "было отжато - стало нажато": повторный вызов в том же состоянии ничего не делает.
     */
    private void pollInput() {
        // Пока поле ловит текст, опрос клавиш напрямую через GLFW обходит хук:
        // буква напечаталась бы в поле и одновременно открыла чат или меню игры.
        boolean typing = gui.capturingText();

        if (!typing) {
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
                if (gui.capturingText()) {
                    gui.cancelTextCapture();
                } else if (gui.capturingKey()) {
                    gui.cancelKeyCapture();
                } else if (hud.editing()) {
                    hud.stopEditing();
                } else if (gui.visible()) {
                    gui.close();
                } else if (radial.visible()) {
                    radial.close();
                }
            }
            escapeWasDown = escapeDown;

            pollModuleBinds();
        }

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

    /**
     * Клавиши модулей.
     *
     * Переключение работает и когда меню закрыто, и когда открыто - так бинд
     * проверяется сразу, не закрывая настройки. Пока поле ловит текст, опрос
     * стоит: буква не должна выключать модуль.
     */
    private void pollModuleBinds() {
        boolean capturing = gui.capturingKey() || gui.capturingText();
        for (Module module : settings.modules()) {
            // У групп настроек переключателя нет, биндов им тоже не положено.
            if (!module.toggleable()) {
                continue;
            }
            int key = module.bindOption().code();
            if (key == Keys.NONE) {
                bindWas.remove(module.key());
                continue;
            }
            boolean down = !capturing && game.keyDown(key);
            Boolean was = bindWas.get(module.key());
            boolean previous = was != null && was;
            int mode = module.bindModeOption().index();
            boolean should;
            if (mode == 1) {
                should = down;
            } else {
                should = module.on();
                if (down && !previous) {
                    should = !module.on();
                }
            }
            bindWas.put(module.key(), down);
            if (should != module.on()) {
                module.setOn(should);
                onModuleToggled(module, should);
            }
        }
    }

    /** Реакция на переключение модуля клавишей: звук и короткое уведомление. */
    private void onModuleToggled(Module module, boolean on) {
        try {
            // Один цифровой призыв, две высоты: вверх - включили, вниз - выключили.
            game.playSound(Sounds.BIT, on ? 1.3f : 0.8f);
            notifications.push(module.title() + (on ? " - вкл" : " - выкл"),
                    on ? NotificationCenter.Level.SUCCESS : NotificationCenter.Level.INFO, 1_200_000_000L);
        } catch (Throwable ignored) {
            // Украшение, не функция.
        }
    }
}
