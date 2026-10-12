package dev.nimbus.core.hud;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Keys;
import dev.nimbus.core.Settings;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.render.Colors;
import dev.nimbus.core.render.Draw;
import dev.nimbus.core.render.Easing;
import dev.nimbus.core.render.Pixels;
import dev.nimbus.core.render.Ring;
import dev.nimbus.core.render.Shapes;
import dev.nimbus.core.ui.Brand;
import dev.nimbus.core.ui.Theme;

/**
 * Постоянный слой поверх игры: логотип, телеметрия, клавиши, прицел.
 *
 * Всё собрано из одинаковых плашек одной высоты и выровнено по одной сетке.
 * Именно это отличает аккуратный клиент от самодельного: не количество цифр на
 * экране, а то, что они выглядят одним целым.
 *
 * Положение панелей. По умолчанию панели стоят стопкой в выбранном углу, но любую
 * можно выдернуть из стопки и поставить куда угодно в режиме расстановки. Остальные
 * при этом смыкают стопку, а не оставляют дыру.
 *
 * Счётчики кадров и кликов считаются сами: брать их у игры - лишние чужие имена,
 * которые ломаются на каждой новой версии.
 *
 * Геометрия рисуется в физических пикселях, текст - в единицах интерфейса.
 */
public final class Hud {

    private static final int MARGIN = 6;
    private static final int SAFE_INSET_MIN = 12;
    private static final int SAFE_INSET_MAX = 28;

    /** Имя клиента: подпись логотипа, пока игрок не задал свою. */
    private static final String BRAND_DEFAULT = "NIMBUS";

    /** Максимум букв своей подписи: длиннее плашка перестаёт быть плашкой. */
    private static final int BRAND_LABEL_MAX = 18;
    private static final int PAD_X = 7;
    private static final int GAP = 3;
    private static final int RADIUS = 5;

    private static final int KIND_CHIP = 0;
    private static final int KIND_BRAND = 1;
    private static final int KIND_GRAPH = 2;
    private static final int KIND_KEYS = 3;
    private static final int KIND_PANEL = 4;
    private static final int KIND_BAR = 5;

    private static final int SLOTS = 24;

    /**
     * Сколько точек хранит график кадров.
     *
     * Одна точка - один столбец шириной в пиксель, и берётся она раз в {@link #SAMPLE_NANOS}.
     * Столбцов на экране не больше, чем внутренняя ширина самого широкого графика
     * (220 минус рамки), поэтому 240 хватает с запасом на любую ширину, а хранить
     * больше незачем: лишние точки всё равно никогда не нарисуются.
     */
    private static final int HISTORY = 240;

    /** Шаг съёма точки графика: пятая доля секунды на столбец. */
    private static final long SAMPLE_NANOS = 200_000_000L;

    /**
     * Насколько рамка редактора выходит за габарит панели.
     *
     * Тем же запасом расширена и проверка попадания: рамка рисовалась снаружи бокса,
     * а клик считался по самому боксу, и нажатие по видимой рамке промахивалось.
     * Панели перебираются сверху вниз, поэтому наложение запасов решается в пользу
     * верхней панели, а не превращается в неоднозначность.
     */
    private static final int EDGE = 2;

    /** Шаг сетки расстановки в единицах интерфейса. */
    private static final int GRID = 4;

    /** С какого расстояния панель прилипает к направляющей. */
    private static final int SNAP = 5;

    /** Стороны света по часовой стрелке, начиная с юга: так их считает сама игра. */
    private static final String[] FACINGS = {"Ю", "Ю-З", "З", "С-З", "С", "С-В", "В", "Ю-В"};

    private final Settings settings;
    private final HudLayout layout;
    private final HudTelemetry telemetry = new HudTelemetry();
    private HudSnapshot snapshot;

    private final String[] keys = new String[SLOTS];
    private final String[] labels = new String[SLOTS];
    private final String[] values = new String[SLOTS];
    private final int[] colors = new int[SLOTS];
    private final int[] kinds = new int[SLOTS];
    private final int[] boxX = new int[SLOTS];
    private final int[] boxY = new int[SLOTS];
    private final int[] boxW = new int[SLOTS];
    private final int[] boxH = new int[SLOTS];
    private int blocks;

    private int fps;

    private final int[] history = new int[HISTORY];
    private int historyCount;
    private int historyHead;
    private long lastSample;
    private boolean sampled;

    /**
     * Кольцевой буфер моментов нажатия: CPS считается по окну в секунду.
     *
     * Заполненность считается отдельно, а не по нулю в ячейке: nanoTime не обязан
     * начинаться с нуля и в принципе может его вернуть, и тогда клик просто исчезал бы.
     */
    private final long[][] clicks = new long[2][32];
    private final int[] clickHead = new int[2];
    private final int[] clickFilled = new int[2];
    private final boolean[] mouseWas = new boolean[2];

    private final float[] keyGlow = new float[8];
    private long lastFrame;
    private boolean framed;

    // Данные панелей боя: снимаются один раз за кадр, читаются при отрисовке.
    private String targetName = "";
    private float targetHealth;
    private float targetMaxHealth;
    private float targetFade;
    private float targetDistance;

    // Скорость: экспоненциальное среднее по горизонтальному перемещению.
    private float speedValue;
    private double speedPrevX;
    private double speedPrevZ;
    private boolean speedReady;

    // Режим расстановки
    private boolean editing;
    private float editFade;
    private String dragKey;
    private double dragOffsetX;
    private double dragOffsetY;
    private String selectedKey;
    private boolean editPreviousLeft;
    private boolean editPreviousRight;
    private int guideX = Integer.MIN_VALUE;
    private int guideY = Integer.MIN_VALUE;

    public Hud(Settings settings) {
        this(settings, new HudLayout());
    }

    /**
     * Явная раскладка: нужна офлайн-проверке.
     *
     * Без этого шва проверка отрисовки писала бы в настоящий файл игрока и стирала
     * бы его расстановку панелей при каждом запуске сборки.
     */
    public Hud(Settings settings, HudLayout layout) {
        this.settings = settings;
        this.layout = layout;
        layout.load();
    }

    public int fps() {
        return fps;
    }

    /** Открыт ли режим расстановки панелей. */
    public boolean editing() {
        return editing;
    }

    public void startEditing() {
        editing = true;
        dragKey = null;
        selectedKey = null;
        editPreviousLeft = true;
        editPreviousRight = true;
    }

    public void stopEditing() {
        if (!editing) {
            return;
        }
        editing = false;
        dragKey = null;
        selectedKey = null;
        layout.saveIfDirty();
    }

    /** Клики в секунду по кнопке мыши. */
    public int cps(int button) {
        long now = System.nanoTime();
        long[] buffer = clicks[button];
        int filled = clickFilled[button];
        int count = 0;
        for (int i = 0; i < filled; i++) {
            if (now - buffer[i] <= 1_000_000_000L) {
                count++;
            }
        }
        return count;
    }

    public void render(GameBridge game, String version, boolean menuOpen) {
        long now = System.nanoTime();
        // Первый кадр идёт без шага времени: разница с нулём дала бы скачок анимаций
        // на всю длину аптайма, а сам нуль - законное значение таймера.
        float delta = framed ? Math.min(0.1f, (now - lastFrame) / 1_000_000_000f) : 0f;
        lastFrame = now;
        framed = true;

        trackClicks(game, now, menuOpen);
        snapshot = telemetry.sample(game, cps(0), cps(1));
        fps = snapshot.fps();
        sampleHistory(now);

        int accent = settings.accent();
        int surface = settings.surfaceIndex();

        editFade = Easing.approach(editFade, editing ? 1f : 0f, 0.06f, delta);

        build(game, version, accent, delta);
        place(game);

        if (editing) {
            // Ввод считается до отрисовки: иначе панель отстаёт от курсора на кадр.
            handleEditing(game);
            drawEditorBackground(game, accent);
        }

        for (int i = 0; i < blocks; i++) {
            drawBlock(game, i, boxX[i], boxY[i], boxW[i], boxH[i], accent, surface, delta);
        }

        if (editFade > 0.004f) {
            drawEditorForeground(game, accent);
        }

        if (!editing && settings.module("crosshair").on() && game.inWorld() && !menuOpen) {
            drawCrosshair(game, accent);
        }

        // Кольцо кулдауна живёт у прицела, а не в стопке: взгляд и так там.
        if (!editing && game.inWorld() && !menuOpen) {
            Module attackcd = settings.moduleOrNull("attackcd");
            if (attackcd != null && attackcd.on() && attackcd.choice("style").index() == 1) {
                drawAttackRing(game, accent);
            }
        }
    }

    // ------------------------------------------------------------------ состав панелей

    private void build(GameBridge game, String version, int accent, float delta) {
        blocks = 0;
        boolean labelled = settings.hudLabels();

        Module watermark = settings.module("watermark");
        if (watermark.on()) {
            add(KIND_BRAND, "watermark", brandLabel(), watermark.bool("version").get() ? version : "", accent);
        }

        HudSnapshot data = snapshot;
        Module fpsModule = settings.module("fps");
        if (fpsModule.on()) {
            int currentFps = data == null ? fps : data.fps();
            int color = fpsModule.bool("colorize").get() ? gradeFps(currentFps) : Theme.TEXT;
            String value = Integer.toString(currentFps);
            if (fpsModule.bool("low").get()) {
                value += " / " + (data == null ? currentFps : data.onePercentLow());
            }
            if (fpsModule.bool("frameTime").get() && data != null) {
                value += " · " + formatFrameTime(data.frameTimeMs());
            }
            add(KIND_CHIP, "fps", labelled ? "FPS" : "", labelled ? value : value + " fps", color);
        }

        Module cpsModule = settings.module("cps");
        if (cpsModule.on()) {
            int mode = cpsModule.choice("mode").index();
            int left = data == null ? cps(0) : data.leftCps();
            int right = data == null ? cps(1) : data.rightCps();
            String value;
            if (mode == 0) {
                value = Integer.toString(left);
            } else if (mode == 1) {
                value = Integer.toString(right);
            } else {
                value = left + " / " + right;
            }
            add(KIND_CHIP, "cps", labelled ? "CPS" : "", labelled ? value : value + " cps", Theme.TEXT);
        }

        Module coords = settings.module("coords");
        if (coords.on()) {
            String text = coordinates(data, coords);
            dev.nimbus.core.NimbusCore core = dev.nimbus.core.NimbusCore.instance();
            if (core != null && core.privacy("coords")) {
                text = "•• •• ••";
            }
            add(KIND_CHIP, "coords", labelled ? "XYZ" : "", text, Theme.TEXT);
        }

        Module clock = settings.module("clock");
        if (clock.on()) {
            add(KIND_CHIP, "clock", "", clock(data, clock), Theme.TEXT);
        }

        Module session = settings.module("session");
        if (session.on()) {
            add(KIND_CHIP, "session", labelled ? "Сессия" : "", session(data), Theme.TEXT);
        }

        Module ping = settings.module("ping");
        if (ping.on()) {
            int value = data == null ? game.ping() : data.ping();
            int color = ping.bool("colorize").get() ? gradePing(value) : Theme.TEXT;
            String text = value < 0 ? "—" : value + " мс";
            add(KIND_CHIP, "ping", labelled ? "Пинг" : "", text, color);
        }

        Module memory = settings.module("memory");
        if (memory.on() && data != null) {
            add(KIND_CHIP, "memory", labelled ? "Память" : "", memory(data, memory), Theme.TEXT);
        }

        if (game.inWorld()) {
            addCombatPanels(game, accent, delta);
        }

        if (settings.module("graph").on()) {
            add(KIND_GRAPH, "graph", "", "", accent);
        }

        if (settings.module("keystrokes").on()) {
            add(KIND_KEYS, "keystrokes", "", "", accent);
        }
    }

    private void add(int kind, String key, String label, String value, int color) {
        if (blocks >= SLOTS) {
            return;
        }
        kinds[blocks] = kind;
        keys[blocks] = key;
        labels[blocks] = label;
        values[blocks] = value;
        colors[blocks] = color;
        blocks++;
    }

    /**
     * Боевые и статусные панели: цель, кулдаун, здоровье, броня, эффекты, скорость, сервер.
     *
     * Данные берутся из моста снимком на кадр, поэтому порядок не важен: панели
     * делят один снимок эффектов и брони между собой. Панель без данных просто
     * не добавляется - пустых плашек на экране не бывает.
     */
    private void addCombatPanels(GameBridge game, int accent, float delta) {
        dev.nimbus.core.NimbusCore core = dev.nimbus.core.NimbusCore.instance();

        // --- цель
        Module targethud = settings.moduleOrNull("targethud");
        if (targethud != null && targethud.on() && core != null) {
            long holdMs = (long) (targethud.slider("hold").get() * 1000f);
            targetFade = core.combat().fade(holdMs);
            dev.nimbus.bridge.TargetInfo info = targetFade > 0f
                    ? game.targetInfo(core.combat().target())
                    : null;
            if (info != null && targetFade > 0f) {
                targetName = info.name();
                targetHealth = info.health();
                targetMaxHealth = info.maxHealth();
                targetDistance = (float) Math.sqrt(
                        Math.pow(game.playerX() - info.x(), 2)
                                + Math.pow(game.playerY() - info.y(), 2)
                                + Math.pow(game.playerZ() - info.z(), 2)
                );
                add(KIND_PANEL, "targethud", "", "", accent);
            } else {
                targetFade = 0f;
            }
        }

        // --- кулдаун удара полосой (кольцо рисуется у прицела отдельно)
        Module attackcd = settings.moduleOrNull("attackcd");
        if (attackcd != null && attackcd.on() && attackcd.choice("style").index() == 0) {
            int percent = game.attackStrengthPercent();
            if (!(attackcd.bool("hideFull").get() && percent >= 100)) {
                add(KIND_BAR, "attackcd", labelled() ? "Замах" : "",
                        percent >= 100 ? "готов" : percent + "%",
                        percent >= 100 ? Theme.GOOD : Theme.TEXT);
            }
        }

        // --- здоровье
        Module health = settings.moduleOrNull("health");
        if (health != null && health.on() && game.playerMaxHealth() > 0f) {
            add(KIND_PANEL, "health", "", "", accent);
        }

        // --- броня
        Module armor = settings.moduleOrNull("armor");
        if (armor != null && armor.on() && game.armorCount() > 0) {
            add(KIND_PANEL, "armor", "", "", accent);
        }

        // --- эффекты
        Module potions = settings.moduleOrNull("potions");
        if (potions != null && potions.on() && game.effectCount() > 0) {
            add(KIND_PANEL, "potions", "", "", accent);
        }

        // --- скорость
        Module speed = settings.moduleOrNull("speed");
        if (speed != null && speed.on()) {
            add(KIND_CHIP, "speed", labelled() ? "Скорость" : "", speedChip(game, delta), Theme.TEXT);
        }

        // --- сервер
        Module server = settings.moduleOrNull("server");
        if (server != null && server.on()) {
            String address = game.serverAddress();
            dev.nimbus.core.NimbusCore instance = core;
            if (instance != null && instance.privacy("server")) {
                address = "скрыто";
            }
            String text = address.isEmpty() ? "Одиночная игра" : address;
            if (server.bool("ping").get() && !address.isEmpty()) {
                int ping = game.ping();
                if (ping >= 0) {
                    text += " · " + ping + " мс";
                }
            }
            add(KIND_CHIP, "server", "", text, Theme.TEXT);
        }
    }

    private boolean labelled() {
        return settings.hudLabels();
    }

    /** Скорость в блоках в секунду, сглаженная, чтобы не дёргалась на каждом кадре. */
    private String speedChip(GameBridge game, float delta) {
        if (delta <= 0f) {
            return Math.round(speedValue) + " б/с";
        }
        double x = game.playerX();
        double z = game.playerZ();
        if (!speedReady) {
            speedPrevX = x;
            speedPrevZ = z;
            speedReady = true;
        }
        double dx = x - speedPrevX;
        double dz = z - speedPrevZ;
        speedPrevX = x;
        speedPrevZ = z;
        float instant = (float) (Math.sqrt(dx * dx + dz * dz) / delta);
        speedValue += (instant - speedValue) * 0.18f;
        if (speedValue < 0.05f) {
            speedValue = 0f;
        }
        return Math.round(speedValue) + " б/с";
    }

    /**
     * Расстановка панелей.
     *
     * Сначала все не тронутые вручную панели выстраиваются стопкой в углу, потом
     * перенесённые садятся на свои места. Порядок важен: иначе перенесённая панель
     * оставляла бы за собой пустое место в стопке.
     */
    private void place(GameBridge game) {
        int chipHeight = game.textHeight() + 8;
        int corner = settings.hudCorner();
        boolean right = corner == 1 || corner == 3;
        boolean bottom = corner == 2 || corner == 3;
        boolean row = settings.hudRow();

        int inset = safeInset(game.screenWidth(), game.screenHeight());
        int cursorX = right ? game.screenWidth() - inset : inset;
        int cursorY = bottom ? game.screenHeight() - inset : inset;

        for (int i = 0; i < blocks; i++) {
            int width = blockWidth(game, i);
            int height = blockHeight(game, i, chipHeight);
            boxW[i] = width;
            boxH[i] = height;

            float[] custom = layout.get(keys[i]);
            if (custom != null) {
                boxX[i] = Math.round(custom[0] * Math.max(0, game.screenWidth() - width));
                boxY[i] = Math.round(custom[1] * Math.max(0, game.screenHeight() - height));
                continue;
            }

            if (kinds[i] == KIND_KEYS) {
                // У клавиш есть свои ползунки положения - они и есть положение по умолчанию.
                Module module = settings.module("keystrokes");
                boxX[i] = Math.round((game.screenWidth() - width) * module.slider("x").get() / 100f);
                boxY[i] = Math.round((game.screenHeight() - height) * module.slider("y").get() / 100f);
                continue;
            }

            boxX[i] = right ? cursorX - width : cursorX;
            boxY[i] = bottom ? cursorY - height : cursorY;
            if (row) {
                cursorX += right ? -(width + GAP) : width + GAP;
            } else {
                cursorY += bottom ? -(height + GAP) : height + GAP;
            }
        }

        for (int i = 0; i < blocks; i++) {
            int minX = Math.min(inset, Math.max(0, game.screenWidth() - boxW[i]));
            int minY = Math.min(inset, Math.max(0, game.screenHeight() - boxH[i]));
            int maxX = Math.max(0, game.screenWidth() - inset - boxW[i]);
            int maxY = Math.max(0, game.screenHeight() - inset - boxH[i]);
            boxX[i] = Math.max(minX, Math.min(maxX, boxX[i]));
            boxY[i] = Math.max(minY, Math.min(maxY, boxY[i]));
        }
    }

    private int blockWidth(GameBridge game, int index) {
        if (kinds[index] == KIND_GRAPH) {
            return settings.module("graph").slider("width").asInt();
        }
        if (kinds[index] == KIND_KEYS) {
            return 18 * 3 + 2 * 2;
        }
        if (kinds[index] == KIND_BRAND) {
            return brandWidth(game, values[index]);
        }
        if (kinds[index] == KIND_BAR) {
            int width = 66 + PAD_X * 2;
            if (!labels[index].isEmpty()) {
                width += game.textWidth(labels[index]) + 5;
            }
            if (settings.hudAccentBar()) {
                width += 4;
            }
            return width;
        }
        if (kinds[index] == KIND_PANEL) {
            return panelWidth(game, keys[index]);
        }
        int width = PAD_X * 2 + game.textWidth(values[index]);
        if (!labels[index].isEmpty()) {
            width += game.textWidth(labels[index]) + 5;
        }
        if (settings.hudAccentBar()) {
            width += 4;
        }
        return width;
    }

    private int blockHeight(GameBridge game, int index, int chipHeight) {
        if (kinds[index] == KIND_GRAPH) {
            return settings.module("graph").slider("height").asInt();
        }
        if (kinds[index] == KIND_KEYS) {
            Module module = settings.module("keystrokes");
            int size = 18;
            int gap = 2;
            return size * 2 + gap
                    + (module.bool("mouse").get() ? size + gap : 0)
                    + (module.bool("space").get() ? 9 + gap : 0);
        }
        if (kinds[index] == KIND_BAR) {
            return chipHeight + 5;
        }
        if (kinds[index] == KIND_PANEL) {
            return panelHeight(game, keys[index]);
        }
        return chipHeight;
    }

    /** Ширина многострочной панели: текст строк плюс общий размер полос. */
    private int panelWidth(GameBridge game, String key) {
        int minBar = 54;
        switch (key) {
            case "targethud": {
                int width = game.textWidth(targetName.isEmpty() ? "цель" : targetName)
                        + (targetDistance > 0 ? game.textWidth(" · " + Math.round(targetDistance) + " б") : 0);
                return Math.max(width, minBar + game.textWidth("100")) + PAD_X * 2;
            }
            case "health": {
                return Math.max(minBar + game.textWidth("20"), game.textWidth("Броня 20")) + PAD_X * 2;
            }
            case "armor": {
                int widest = minBar + game.textWidth("100%");
                for (int i = 0; i < game.armorCount(); i++) {
                    dev.nimbus.bridge.ArmorPiece piece = game.armor(i);
                    if (piece != null) {
                        widest = Math.max(widest, game.textWidth(piece.name()) + minBar + game.textWidth("100%") + 6);
                    }
                }
                return widest + PAD_X * 2;
            }
            default: {
                // Эффекты: "Название II" слева, время справа.
                int widest = minBar;
                for (int i = 0; i < game.effectCount(); i++) {
                    dev.nimbus.bridge.EffectInfo effect = game.effect(i);
                    if (effect != null) {
                        String left = effect.name() + roman(effect.amplifier());
                        String right = effect.seconds() < 0 ? "—" : effect.seconds() + " с";
                        widest = Math.max(widest, game.textWidth(left) + 8 + game.textWidth(right));
                    }
                }
                return widest + PAD_X * 2;
            }
        }
    }

    /** Высота многострочной панели по числу строк. */
    private int panelHeight(GameBridge game, String key) {
        int th = game.textHeight();
        int rows;
        switch (key) {
            case "targethud":
                rows = 2;
                break;
            case "health": {
                Module module = settings.moduleOrNull("health");
                rows = 1;
                if (module != null && module.bool("absorption").get()) {
                    rows++;
                }
                if (module != null && module.bool("armorPoints").get()) {
                    rows++;
                }
                break;
            }
            case "armor":
                rows = Math.max(1, game.armorCount());
                break;
            default:
                rows = Math.max(1, game.effectCount());
                break;
        }
        return 6 + rows * (th + 4) + 2;
    }

    private int brandWidth(GameBridge game, String version) {
        int style = settings.module("watermark").choice("style").index();
        if (style == 2) {
            return game.textHeight() + 14;
        }
        int width = game.textWidth(brandLabel()) + 5 + PAD_X * 2 + 14;
        if (!version.isEmpty()) {
            width += game.textWidth(version) + 6;
        }
        if (style == 1) {
            width -= PAD_X * 2;
        }
        return width;
    }

    /**
     * Подпись логотипа: своя, если игрок её задал, иначе имя клиента.
     *
     * Строка приходит из файла настроек, поэтому доверять ей нельзя: знак секции
     * в шрифте игры - это код цвета, а управляющие символы рисуются мусором.
     * Остаются только видимые буквы, и длина режется: подпись - украшение плашки,
     * а не место для абзаца.
     */
    private String brandLabel() {
        String raw = settings.module("watermark").text("label").get();
        if (raw.isEmpty()) {
            return BRAND_DEFAULT;
        }
        StringBuilder clean = new StringBuilder(BRAND_LABEL_MAX);
        for (int i = 0; i < raw.length() && clean.length() < BRAND_LABEL_MAX; i++) {
            char symbol = raw.charAt(i);
            if (symbol < ' ' || symbol == 0x7F || symbol == 0xA7) {
                continue;
            }
            clean.append(symbol);
        }
        // Одни пробелы - та же пустота: показываем имя клиента.
        String result = clean.toString().trim();
        return result.isEmpty() ? BRAND_DEFAULT : result;
    }

    private void drawBlock(GameBridge game, int index, int x, int y, int width, int height, int accent, int surface, float delta) {
        if (kinds[index] == KIND_GRAPH) {
            drawGraph(game, x, y, width, height, accent, surface);
            return;
        }
        if (kinds[index] == KIND_KEYS) {
            drawKeystrokes(game, x, y, width, accent, surface, delta);
            return;
        }
        if (kinds[index] == KIND_BRAND) {
            drawBrand(game, x, y, width, height, values[index], accent, surface);
            return;
        }
        if (kinds[index] == KIND_PANEL) {
            drawPanel(game, index, x, y, width, height, accent, surface);
            return;
        }
        if (kinds[index] == KIND_BAR) {
            drawCooldownBar(game, index, x, y, width, height, accent, surface);
            return;
        }

        float opacity = settings.hudOpacity();
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
        Draw.roundedOutline(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                RADIUS * s,
                Math.max(1, s / 2),
                Colors.fade(Theme.LINE, opacity)
        );
        if (settings.hudAccentBar()) {
            Draw.roundedRect(game, (x + 4) * s, (y + 4) * s, Math.max(1, s), (height - 8) * s, Math.max(1, s / 2), accent);
        }
        game.popScale();

        int textX = x + PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int textY = y + (height - game.textHeight()) / 2 + 1;
        if (!labels[index].isEmpty()) {
            game.drawText(labels[index], textX, textY, Theme.TEXT_DIM, true);
            textX += game.textWidth(labels[index]) + 5;
        }
        game.drawText(values[index], textX, textY, colors[index], true);
    }

    /** Общий фон многострочных панелей: тот же чип, что у однострочных. */
    private void panelBase(GameBridge game, int x, int y, int width, int height, int accent, int surface) {
        panelBase(game, x, y, width, height, accent, surface, 1f);
    }

    private void panelBase(
            GameBridge game,
            int x,
            int y,
            int width,
            int height,
            int accent,
            int surface,
            float opacityMul
    ) {
        float opacity = settings.hudOpacity() * opacityMul;
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
        Draw.roundedOutline(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                RADIUS * s,
                Math.max(1, s / 2),
                Colors.fade(Theme.LINE, opacity)
        );
        if (settings.hudAccentBar()) {
            Draw.roundedRect(game, (x + 4) * s, (y + 4) * s, Math.max(1, s), (height - 8) * s, Math.max(1, s / 2),
                    Colors.fade(accent, opacityMul));
        }
        game.popScale();
    }

    private void drawPanel(GameBridge game, int index, int x, int y, int width, int height, int accent, int surface) {
        // Панель цели гаснет вместе с данными, а не исчезает скачком.
        float opacityMul = "targethud".equals(keys[index]) ? Math.max(0.05f, targetFade) : 1f;
        panelBase(game, x, y, width, height, accent, surface, opacityMul);
        switch (keys[index]) {
            case "targethud":
                drawTargetPanel(game, x, y, width, accent);
                break;
            case "health":
                drawHealthPanel(game, x, y, width, accent);
                break;
            case "armor":
                drawArmorPanel(game, x, y, width, accent);
                break;
            default:
                drawPotionsPanel(game, x, y, width, accent);
                break;
        }
    }

    /** Строка панели: подпись слева, значение справа. */
    private void panelRow(GameBridge game, int x, int y, int width, String left, String right, int leftColor, int rightColor) {
        int th = game.textHeight();
        game.drawText(left, x, y, leftColor, true);
        if (right != null && !right.isEmpty()) {
            game.drawText(right, x + width - game.textWidth(right), y, rightColor, true);
        }
    }

    /** Полоса состояния: фон, заливка с мягким хвостом и тонкая рамка. */
    private void panelBar(GameBridge game, int x, int y, int width, float fraction, int color) {
        int th = game.textHeight();
        int barHeight = Math.max(2, th / 3);
        int barY = y + th - barHeight - 1;
        float frac = Math.max(0f, Math.min(1f, fraction));
        game.fill(x, barY, width, barHeight, 0x2EFFFFFF);
        if (frac > 0f) {
            game.fillGradient(x, barY, Math.max(1, Math.round(width * frac)), barHeight,
                    Colors.withAlpha(color, 0xE6), Colors.withAlpha(color, 0x9A));
        }
    }

    private void drawTargetPanel(GameBridge game, int x, int y, int width, int accent) {
        int padX = PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int th = game.textHeight();
        float fade = targetFade;
        Module module = settings.module("targethud");
        boolean distance = module.bool("distance").get();

        String name = targetName.isEmpty() ? "цель" : targetName;
        String right = distance ? Math.round(targetDistance) + " б" : "";
        int rowY = y + 5;
        panelRow(game, x + padX, rowY, width - padX * 2, name, right,
                Colors.fade(Theme.TEXT, fade), Colors.fade(Theme.TEXT_DIM, fade));
        float frac = targetMaxHealth > 0f ? targetHealth / targetMaxHealth : 0f;
        panelBar(game, x + padX, rowY + th + 2, width - padX * 2, frac,
                frac > 0.5f ? Theme.GOOD : (frac > 0.25f ? Theme.WARN : Theme.DANGER));
    }

    private void drawHealthPanel(GameBridge game, int x, int y, int width, int accent) {
        Module module = settings.module("health");
        int padX = PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int th = game.textHeight();
        int inner = width - padX * 2;
        float max = game.playerMaxHealth();
        float hp = game.playerHealth();
        int rowY = y + 5;

        panelRow(game, x + padX, rowY, inner, "Здоровье", trim(hp) + (max > 0 ? " / " + trim(max) : ""),
                Theme.TEXT_DIM, hp <= max * 0.3f ? Theme.DANGER : Theme.TEXT);
        panelBar(game, x + padX, rowY + th + 2, inner, max > 0 ? hp / max : 0f, Theme.GOOD);
        rowY += th + 4;

        if (module.bool("absorption").get()) {
            float abs = game.playerAbsorption();
            panelRow(game, x + padX, rowY, inner, "Поглощение", abs > 0 ? trim(abs) : "",
                    Theme.TEXT_DIM, Theme.WARN);
            panelBar(game, x + padX, rowY + th + 2, inner, max > 0 ? abs / max : 0f, Theme.WARN);
            rowY += th + 4;
        }
        if (module.bool("armorPoints").get()) {
            int points = game.playerArmorPoints();
            panelRow(game, x + padX, rowY, inner, "Броня", points > 0 ? Integer.toString(points) : "",
                    Theme.TEXT_DIM, points >= 15 ? accent : Theme.TEXT);
        }
    }

    private void drawArmorPanel(GameBridge game, int x, int y, int width, int accent) {
        Module module = settings.module("armor");
        boolean percent = module.bool("percent").get();
        boolean warn = module.bool("warn").get();
        int padX = PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int th = game.textHeight();
        int inner = width - padX * 2;
        int rowY = y + 5;

        for (int i = 0; i < game.armorCount(); i++) {
            dev.nimbus.bridge.ArmorPiece piece = game.armor(i);
            if (piece == null) {
                continue;
            }
            float integrity = piece.integrity();
            int left = Math.round(integrity * 100f);
            String value = percent ? left + "%" : piece.maxDurability() - piece.durability() + "/" + piece.maxDurability();
            boolean low = warn && integrity < 0.15f;
            panelRow(game, x + padX, rowY, inner, piece.name(), value,
                    Theme.TEXT_DIM, low ? Theme.DANGER : Theme.TEXT);
            panelBar(game, x + padX, rowY + th + 2, inner, integrity, low ? Theme.DANGER : accent);
            rowY += th + 4;
        }
    }

    private void drawPotionsPanel(GameBridge game, int x, int y, int width, int accent) {
        Module module = settings.module("potions");
        boolean seconds = module.bool("seconds").get();
        boolean levels = module.bool("level").get();
        int padX = PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int inner = width - padX * 2;
        int rowY = y + 5;

        for (int i = 0; i < game.effectCount(); i++) {
            dev.nimbus.bridge.EffectInfo effect = game.effect(i);
            if (effect == null) {
                continue;
            }
            String name = effect.name() + (levels ? roman(effect.amplifier()) : "");
            String time = seconds && effect.seconds() >= 0 ? effect.seconds() + " с" : (effect.seconds() < 0 ? "—" : "");
            int color = effect.harmful() ? Theme.DANGER : Theme.GOOD;
            int dotX = x + padX + 1;
            game.fill(dotX, rowY + game.textHeight() / 2 - 1, 2, 2, color);
            panelRow(game, dotX + 6, rowY, inner - 6, name, time, Theme.TEXT, time.isEmpty() ? Theme.TEXT : Theme.TEXT_DIM);
            rowY += game.textHeight() + 4;
        }
    }

    /** Кулдаун удара полосой в стопке. */
    private void drawCooldownBar(GameBridge game, int index, int x, int y, int width, int height, int accent, int surface) {
        panelBase(game, x, y, width, height, accent, surface);
        int padX = PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int th = game.textHeight();
        int inner = width - padX * 2;
        int percent = game.attackStrengthPercent();
        int textY = y + 4;
        if (!labels[index].isEmpty()) {
            game.drawText(labels[index], x + padX, textY, Theme.TEXT_DIM, true);
        }
        game.drawText(values[index], x + width - padX - game.textWidth(values[index]), textY, colors[index], true);
        panelBar(game, x + padX, textY + th + 1, inner, percent / 100f,
                percent >= 100 ? Theme.GOOD : (percent < 30f ? Theme.DANGER : accent));
    }

    /** Кольцо кулдауна вокруг прицела: сектор растёт по часовой стрелке сверху. */
    private void drawAttackRing(GameBridge game, int accent) {
        int percent = game.attackStrengthPercent();
        Module module = settings.moduleOrNull("attackcd");
        if (module != null && module.bool("hideFull").get() && percent >= 100) {
            return;
        }
        int s = Pixels.scale(game);
        float cx = (game.screenWidth() * s) / 2f;
        float cy = (game.screenHeight() * s) / 2f;
        float radius = 10f * s;
        float thickness = 2.5f * s;
        float opacity = settings.hudOpacity();
        Ring.ring(game, cx, cy, radius, thickness, Colors.fade(0x46FFFFFF, opacity));
        float sweep = 359.9f * (percent / 100f);
        if (sweep > 0.5f) {
            Ring.sector(game, cx, cy, radius - thickness, radius, -90f, sweep,
                    Colors.fade(percent >= 100 ? Theme.GOOD : Colors.withAlpha(accent, 0xF2), opacity));
        }
    }

    private static String trim(float value) {
        float rounded = Math.round(value * 10f) / 10f;
        if (rounded == Math.round(rounded)) {
            return Integer.toString(Math.round(rounded));
        }
        return String.format(java.util.Locale.ROOT, "%.1f", rounded);
    }

    /** Римский суффикс уровня эффекта: ноль - без суффикса, дальше I, II, III... */
    private static String roman(int level) {
        switch (level) {
            case 0:
                return "";
            case 1:
                return " II";
            case 2:
                return " III";
            case 3:
                return " IV";
            case 4:
                return " V";
            default:
                return " +" + level;
        }
    }

    /**
     * Логотип.
     *
     * Значок - настоящая иконка лаунчера. Три стиля не прихоть: на стриме нужна
     * крупная подпись, на турнире - самая незаметная из возможных.
     */
    private void drawBrand(GameBridge game, int x, int y, int width, int height, String version, int accent, int surface) {
        int style = settings.module("watermark").choice("style").index();
        float opacity = settings.hudOpacity();
        int s = Pixels.scale(game);

        if (style != 1) {
            game.pushScale(1f / s);
            Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
            Draw.roundedOutline(
                    game,
                    x * s,
                    y * s,
                    width * s,
                    height * s,
                    RADIUS * s,
                    Math.max(1, s / 2),
                    Colors.fade(Colors.withAlpha(accent, 0x40), opacity)
            );
            game.popScale();
        }

        int centerY = y + height / 2;
        int textY = y + (height - game.textHeight()) / 2 + 1;

        if (style == 2) {
            int mark = Math.min(height - 4, 14);
            Brand.draw(game, x + (width - mark) / 2, centerY - mark / 2, mark, opacity, accent);
            return;
        }

        int markSize = Math.min(height - 4, 12);
        int textX = x + (style == 1 ? 0 : PAD_X);
        Brand.draw(game, textX, centerY - markSize / 2, markSize, opacity, accent);
        textX += markSize + 5;

        // Одна отрисовка вместо двух со сдвигом: самодельная жирность слепляет буквы
        // в белое пятно. Читаемость даёт тень и воздух между буквами, а не толщина.
        int titleColor = Colors.fade(Colors.mix(Theme.TEXT_DIM, Theme.TEXT, 0.5f), opacity);
        String title = brandLabel();
        for (int i = 0; i < title.length(); i++) {
            String letter = title.substring(i, i + 1);
            game.drawText(letter, textX, textY, titleColor, true);
            textX += game.textWidth(letter) + 1;
        }
        textX += 5;
        if (!version.isEmpty()) {
            game.drawText(version, textX, textY, Colors.fade(Colors.withAlpha(accent, 0xFF), opacity), true);
        }
    }

    // ------------------------------------------------------------------ режим расстановки

    private void handleEditing(GameBridge game) {
        boolean left = game.mouseDown(Keys.MOUSE_LEFT);
        boolean right = game.mouseDown(Keys.MOUSE_RIGHT);
        boolean leftClick = left && !editPreviousLeft;
        boolean rightClick = right && !editPreviousRight;
        editPreviousLeft = left;
        editPreviousRight = right;

        int mouseX = game.mouseX();
        int mouseY = game.mouseY();
        double preciseX = game.mouseXPrecise();
        double preciseY = game.mouseYPrecise();

        int hovered = blockAt(mouseX, mouseY);
        if (leftClick && hovered < 0) {
            selectedKey = null;
        }

        if (rightClick && hovered >= 0) {
            // Правая кнопка возвращает панель в стопку.
            layout.reset(keys[hovered]);
            selectedKey = keys[hovered];
            dragKey = null;
            layout.saveIfDirty();
            return;
        }

        if (leftClick && hovered >= 0) {
            selectedKey = keys[hovered];
            dragKey = keys[hovered];
            dragOffsetX = preciseX - boxX[hovered];
            dragOffsetY = preciseY - boxY[hovered];
        }
        if (!left) {
            if (dragKey != null) {
                dragKey = null;
                layout.saveIfDirty();
            }
            guideX = Integer.MIN_VALUE;
            guideY = Integer.MIN_VALUE;
            return;
        }
        if (dragKey == null) {
            return;
        }

        int index = indexOf(dragKey);
        if (index < 0) {
            dragKey = null;
            return;
        }

        int width = boxW[index];
        int height = boxH[index];
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();

        int x = (int) Math.round(preciseX - dragOffsetX);
        int y = (int) Math.round(preciseY - dragOffsetY);

        // Сначала сетка, потом направляющие: прилипание должно перебивать шаг сетки,
        // иначе панель никогда не встанет ровно по центру нечётного экрана.
        x = Math.round(x / (float) GRID) * GRID;
        y = Math.round(y / (float) GRID) * GRID;

        guideX = Integer.MIN_VALUE;
        guideY = Integer.MIN_VALUE;

        int inset = safeInset(screenWidth, screenHeight);
        int centerX = (screenWidth - width) / 2;
        int centerY = (screenHeight - height) / 2;
        int minX = Math.min(inset, Math.max(0, screenWidth - width));
        int minY = Math.min(inset, Math.max(0, screenHeight - height));
        int maxX = Math.max(0, screenWidth - inset - width);
        int maxY = Math.max(0, screenHeight - inset - height);
        if (Math.abs(x - centerX) <= SNAP) {
            x = centerX;
            guideX = screenWidth / 2;
        } else if (Math.abs(x - minX) <= SNAP) {
            x = minX;
        } else if (Math.abs(x - maxX) <= SNAP) {
            x = maxX;
        }
        if (Math.abs(y - centerY) <= SNAP) {
            y = centerY;
            guideY = screenHeight / 2;
        } else if (Math.abs(y - minY) <= SNAP) {
            y = minY;
        } else if (Math.abs(y - maxY) <= SNAP) {
            y = maxY;
        }

        // Выравнивание по соседним панелям: ровные колонки сами собой не получаются.
        for (int i = 0; i < blocks; i++) {
            if (i == index) {
                continue;
            }
            if (Math.abs(x - boxX[i]) <= SNAP) {
                x = boxX[i];
                guideX = x;
            } else if (Math.abs(x + width - (boxX[i] + boxW[i])) <= SNAP) {
                x = boxX[i] + boxW[i] - width;
                guideX = x + width;
            }
            if (Math.abs(y - boxY[i]) <= SNAP) {
                y = boxY[i];
                guideY = y;
            } else if (Math.abs(y + height - (boxY[i] + boxH[i])) <= SNAP) {
                y = boxY[i] + boxH[i] - height;
                guideY = y + height;
            }
        }

        x = Math.max(minX, Math.min(maxX, x));
        y = Math.max(minY, Math.min(maxY, y));

        boxX[index] = x;
        boxY[index] = y;
        layout.set(
                dragKey,
                x / (float) Math.max(1, screenWidth - width),
                y / (float) Math.max(1, screenHeight - height)
        );
    }

    private int blockAt(int x, int y) {
        for (int i = blocks - 1; i >= 0; i--) {
            if (x >= boxX[i] - EDGE && x < boxX[i] + boxW[i] + EDGE
                    && y >= boxY[i] - EDGE && y < boxY[i] + boxH[i] + EDGE) {
                return i;
            }
        }
        return -1;
    }

    /**
     * Короткая подпись панели для редактора.
     *
     * Названия из настроек рассчитаны на меню, где рядом есть описание: "Логотип
     * клиента" или "Раскладка клавиш" не влезут в габарит самой панели. Поэтому у
     * длинных есть короткий вариант, а всё остальное берётся из настроек - иначе
     * новый модуль показывался бы в редакторе своим внутренним ключом.
     */
    private String panelName(String key) {
        String shortened = shortName(key);
        if (shortened != null) {
            return shortened;
        }
        Module module = settings.moduleOrNull(key);
        return module == null ? key : module.title();
    }

    private static String shortName(String key) {
        if ("watermark".equals(key)) {
            return "Бренд";
        }
        if ("fps".equals(key)) {
            return "FPS";
        }
        if ("cps".equals(key)) {
            return "CPS";
        }
        if ("session".equals(key)) {
            return "Сессия";
        }
        if ("memory".equals(key)) {
            return "Память";
        }
        if ("graph".equals(key)) {
            return "График";
        }
        if ("keystrokes".equals(key)) {
            return "Клавиши";
        }
        if ("targethud".equals(key)) {
            return "Цель";
        }
        if ("attackcd".equals(key)) {
            return "Замах";
        }
        return null;
    }

    private boolean overlaps(int index) {
        for (int i = 0; i < blocks; i++) {
            if (i == index) {
                continue;
            }
            if (boxX[index] < boxX[i] + boxW[i]
                    && boxX[index] + boxW[index] > boxX[i]
                    && boxY[index] < boxY[i] + boxH[i]
                    && boxY[index] + boxH[index] > boxY[i]) {
                return true;
            }
        }
        return false;
    }

    private int indexOf(String key) {
        for (int i = 0; i < blocks; i++) {
            if (keys[i].equals(key)) {
                return i;
            }
        }
        return -1;
    }

    private int safeInset(int screenWidth, int screenHeight) {
        return Math.max(SAFE_INSET_MIN, Math.min(SAFE_INSET_MAX, Math.min(screenWidth, screenHeight) / 18));
    }

    private void drawSafeArea(GameBridge game, int accent, int scale, int screenWidth, int screenHeight) {
        int inset = safeInset(screenWidth, screenHeight);
        int line = Colors.fade(Colors.withAlpha(accent, 0xA0), 0.45f * editFade);
        game.pushScale(1f / scale);
        Draw.roundedOutline(
                game,
                inset * scale,
                inset * scale,
                Math.max(1, screenWidth - inset * 2) * scale,
                Math.max(1, screenHeight - inset * 2) * scale,
                5 * scale,
                Math.max(1, scale / 2),
                line
        );
        game.popScale();
        game.drawText("SAFE AREA", inset + 6, inset + 4, Colors.fade(Theme.TEXT_MUTED, editFade), false);
    }

    private void drawEditorHeader(GameBridge game, int accent, int scale, int screenWidth, int screenHeight, int hovered) {
        String selected = selectedKey == null ? "Нет выбора" : panelName(selectedKey);
        int overlapCount = 0;
        for (int i = 0; i < blocks; i++) {
            if (overlaps(i)) {
                overlapCount++;
            }
        }
        String status;
        int statusColor;
        if (layout.saveFailed()) {
            status = "Не удалось сохранить раскладку";
            statusColor = Theme.DANGER;
        } else {
            status = overlapCount == 0 ? "Сетка чистая" : overlapCount + " панелей пересекаются";
            statusColor = overlapCount == 0 ? Theme.GOOD : Theme.WARN;
        }
        String title = "NIMBUS  /  HUD EDITOR";
        int width = Math.max(1, Math.min(Math.max(1, screenWidth - 20), Math.max(game.textWidth(title) + 24, game.textWidth(selected + "  ·  " + status) + 24)));
        int height = game.textHeight() * 2 + 14;
        int x = (screenWidth - width) / 2;
        int y = 8;
        game.pushScale(1f / scale);
        Draw.roundedRect(game, x * scale, y * scale, width * scale, height * scale, RADIUS * scale, Colors.fade(0xFF0B0E14, 0.94f * editFade));
        Draw.roundedOutline(game, x * scale, y * scale, width * scale, height * scale, RADIUS * scale, Math.max(1, scale / 2), Colors.fade(Colors.withAlpha(accent, 0xB0), editFade));
        game.popScale();
        game.drawText(title, x + 12, y + 5, Colors.fade(Theme.TEXT, editFade), true);
        String detail = Shapes.clip(game, selected + "  ·  " + status, Math.max(1, width - 24));
        game.drawText(detail, x + 12, y + game.textHeight() + 7, Colors.fade(statusColor, editFade), false);
    }

    /** Затемнение и сетка под панелями. */
    private void drawEditorBackground(GameBridge game, int accent) {
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();
        game.fill(0, 0, screenWidth, screenHeight, Colors.fade(0xFF05070B, 0.55f * editFade));

        int s = Pixels.scale(game);
        int step = GRID * 4;
        int line = Colors.fade(0xFFFFFFFF, 0.05f * editFade);
        game.pushScale(1f / s);
        for (int x = 0; x <= screenWidth; x += step) {
            game.fill(x * s, 0, Math.max(1, s / 2), screenHeight * s, line);
        }
        for (int y = 0; y <= screenHeight; y += step) {
            game.fill(0, y * s, screenWidth * s, Math.max(1, s / 2), line);
        }
        // Оси экрана видны всегда: по ним ловится центр.
        game.fill((screenWidth / 2) * s, 0, Math.max(1, s / 2), screenHeight * s, Colors.fade(accent, 0.18f * editFade));
        game.fill(0, (screenHeight / 2) * s, screenWidth * s, Math.max(1, s / 2), Colors.fade(accent, 0.18f * editFade));
        game.popScale();
    }

    /** Рамки панелей, направляющие и подсказка. */
    private void drawEditorForeground(GameBridge game, int accent) {
        int s = Pixels.scale(game);
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();
        int mouseX = game.mouseX();
        int mouseY = game.mouseY();
        int hovered = blockAt(mouseX, mouseY);

        game.pushScale(1f / s);
        for (int i = 0; i < blocks; i++) {
            boolean active = dragKey != null && dragKey.equals(keys[i]);
            boolean selected = selectedKey != null && selectedKey.equals(keys[i]);
            boolean over = i == hovered;
            boolean overlap = overlaps(i);
            int outline = active || selected ? Colors.withAlpha(accent, 0xFF)
                    : (over ? Theme.TEXT_DIM : (overlap ? Theme.WARN : Theme.LINE));
            int color = Colors.fade(outline, editFade);
            if (selected || active) {
                Draw.roundedRect(
                        game,
                        (boxX[i] - 1) * s,
                        (boxY[i] - 1) * s,
                        (boxW[i] + 2) * s,
                        (boxH[i] + 2) * s,
                        4 * s,
                        Colors.fade(Colors.withAlpha(accent, 0x22), editFade)
                );
            }
            Draw.roundedOutline(
                    game,
                    (boxX[i] - EDGE) * s,
                    (boxY[i] - EDGE) * s,
                    (boxW[i] + EDGE * 2) * s,
                    (boxH[i] + EDGE * 2) * s,
                    4 * s,
                    active || selected ? Math.max(1, s) : Math.max(1, s / 2),
                    color
            );
            String name = panelName(keys[i]);
            int labelWidth = game.textWidth(name) + 10;
            int labelHeight = game.textHeight() + 5;
            int labelX = Math.max(2, Math.min(screenWidth - labelWidth - 2, boxX[i]));
            int labelY = boxY[i] - labelHeight - 4;
            if (labelY < 2) {
                // Снизу тоже может не быть места: на низком окне подпись остаётся
                // на экране, пусть и поверх панели, вместо ухода за верхний край.
                labelY = Math.max(2, Math.min(screenHeight - labelHeight - 2, boxY[i] + boxH[i] + 4));
            }
            int labelColor = overlap ? Theme.WARN : (selected || active ? accent : Theme.TEXT_DIM);
            Draw.roundedRect(
                    game,
                    labelX * s,
                    labelY * s,
                    labelWidth * s,
                    labelHeight * s,
                    3 * s,
                    Colors.fade(0xF20B0E14, editFade)
            );
            Draw.roundedOutline(
                    game,
                    labelX * s,
                    labelY * s,
                    labelWidth * s,
                    labelHeight * s,
                    3 * s,
                    Math.max(1, s / 2),
                    Colors.fade(Colors.withAlpha(labelColor, 0xB0), editFade)
            );
            game.drawText(name, labelX + 5, labelY + 3, Colors.fade(labelColor, editFade), selected || active);
        }
        if (guideX != Integer.MIN_VALUE) {
            game.fill(guideX * s, 0, Math.max(1, s / 2), screenHeight * s, Colors.fade(accent, 0.8f * editFade));
        }
        if (guideY != Integer.MIN_VALUE) {
            game.fill(0, guideY * s, screenWidth * s, Math.max(1, s / 2), Colors.fade(accent, 0.8f * editFade));
        }
        game.popScale();

        drawSafeArea(game, accent, s, screenWidth, screenHeight);
        drawEditorHeader(game, accent, s, screenWidth, screenHeight, hovered);

        String hint = "ЛКМ — выбрать и перетащить   ·   ПКМ — сбросить панель   ·   ESC — готово";
        int width = Math.max(1, Math.min(Math.max(1, screenWidth - 20), game.textWidth(hint) + 20));
        int height = game.textHeight() + 10;
        int x = (screenWidth - width) / 2;
        int y = Math.max(42, screenHeight - height - 12);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(0xFF0B0E14, 0.92f * editFade));
        Draw.roundedOutline(game, x * s, y * s, width * s, height * s, RADIUS * s, Math.max(1, s / 2), Colors.fade(Colors.withAlpha(accent, 0x80), editFade));
        game.popScale();
        Draw.textCentered(game, Shapes.clip(game, hint, Math.max(1, width - 20)), screenWidth / 2, y + 5, Colors.fade(Theme.TEXT_DIM, editFade), false);
    }

    // ------------------------------------------------------------------ график

    private void sampleHistory(long now) {
        if (sampled && now - lastSample < SAMPLE_NANOS) {
            return;
        }
        sampled = true;
        lastSample = now;
        history[historyHead] = fps;
        historyHead = (historyHead + 1) % HISTORY;
        if (historyCount < HISTORY) {
            historyCount++;
        }
    }

    private void drawGraph(GameBridge game, int x, int y, int width, int height, int accent, int surface) {
        float opacity = settings.hudOpacity();
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
        Draw.roundedOutline(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                RADIUS * s,
                Math.max(1, s / 2),
                Colors.fade(Theme.LINE, opacity)
        );
        game.popScale();

        int plotX = x + 4;
        int plotY = y + 4;
        int plotWidth = width - 8;
        int plotHeight = height - 8;
        if (plotWidth <= 2 || plotHeight <= 2 || historyCount == 0) {
            return;
        }

        int peak = 60;
        int shown = Math.min(historyCount, plotWidth);
        for (int i = 0; i < shown; i++) {
            peak = Math.max(peak, sample(shown, i));
        }

        boolean fill = settings.module("graph").bool("fill").get();
        int previous = -1;
        for (int i = 0; i < shown; i++) {
            int value = sample(shown, i);
            int columnHeight = Math.max(1, Math.round(value / (float) peak * plotHeight));
            int columnX = plotX + plotWidth - shown + i;
            int columnY = plotY + plotHeight - columnHeight;
            if (fill) {
                game.fillGradient(
                        columnX,
                        columnY,
                        1,
                        columnHeight,
                        Colors.withAlpha(accent, 0x8C),
                        Colors.withAlpha(accent, 0x14)
                );
            }
            game.fill(columnX, columnY, 1, 1, accent);
            if (previous >= 0 && Math.abs(previous - columnY) > 1) {
                // Соединительная стойка: без неё линия рассыпается на точки на резких просадках.
                int top = Math.min(previous, columnY);
                game.fill(columnX, top, 1, Math.abs(previous - columnY), Colors.withAlpha(accent, 0xB0));
            }
            previous = columnY;
        }

        Draw.textRight(game, Integer.toString(peak), x + width - 4, y + 2, Theme.TEXT_MUTED, false);
    }

    private int sample(int shown, int index) {
        int position = (historyHead - shown + index + HISTORY * 2) % HISTORY;
        return history[position];
    }

    // ------------------------------------------------------------------ клавиши

    private void drawKeystrokes(GameBridge game, int x, int y, int blockWidth, int accent, int surface, float delta) {
        Module module = settings.module("keystrokes");
        boolean mouse = module.bool("mouse").get();
        boolean showCps = module.bool("cps").get();
        boolean space = module.bool("space").get();

        int size = 18;
        int gap = 2;

        key(game, 0, x + size + gap, y, size, size, "W", game.keyDown(Keys.W), accent, surface, delta);
        int row = y + size + gap;
        key(game, 1, x, row, size, size, "A", game.keyDown(Keys.A), accent, surface, delta);
        key(game, 2, x + size + gap, row, size, size, "S", game.keyDown(Keys.S), accent, surface, delta);
        key(game, 3, x + (size + gap) * 2, row, size, size, "D", game.keyDown(Keys.D), accent, surface, delta);

        int next = row + size + gap;
        if (mouse) {
            int half = (blockWidth - gap) / 2;
            boolean left = game.mouseDown(Keys.MOUSE_LEFT);
            boolean right = game.mouseDown(Keys.MOUSE_RIGHT);
            key(game, 4, x, next, half, size, showCps ? Integer.toString(cps(0)) : "ЛКМ", left, accent, surface, delta);
            key(game, 5, x + half + gap, next, blockWidth - half - gap, size, showCps ? Integer.toString(cps(1)) : "ПКМ", right, accent, surface, delta);
            next += size + gap;
        }
        if (space) {
            key(game, 6, x, next, blockWidth, 9, "", game.keyDown(Keys.SPACE), accent, surface, delta);
        }
    }

    private void key(
            GameBridge game,
            int slot,
            int x,
            int y,
            int width,
            int height,
            String text,
            boolean pressed,
            int accent,
            int surface,
            float delta
    ) {
        keyGlow[slot] = Easing.approach(keyGlow[slot], pressed ? 1f : 0f, 0.045f, delta);
        float glow = keyGlow[slot];
        float opacity = settings.hudOpacity();

        int background = Colors.mix(Colors.fade(Theme.chip(surface), opacity), Colors.withAlpha(accent, 0xE6), glow);
        int border = Colors.mix(Colors.fade(Theme.LINE, opacity), Colors.withAlpha(accent, 0xFF), glow);

        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, 4 * s, background);
        Draw.roundedOutline(game, x * s, y * s, width * s, height * s, 4 * s, Math.max(1, s / 2), border);
        game.popScale();

        if (!text.isEmpty()) {
            int color = Colors.mix(Theme.TEXT_DIM, 0xFF0B0E14, glow);
            Draw.textCentered(game, text, x + width / 2, y + (height - game.textHeight()) / 2 + 1, color, false);
        }
    }

    // ------------------------------------------------------------------ прицел

    /**
     * Свой прицел.
     *
     * Центр считается в физических пикселях, а не в единицах интерфейса. При масштабе 3
     * одна единица - это три настоящих пикселя, и округление центра уводит прицел
     * в сторону от точки, куда реально смотрит игрок. Толщина подгоняется по чётности
     * экрана: иначе лишний пиксель всегда достаётся одной из сторон.
     */
    private void drawCrosshair(GameBridge game, int accent) {
        Module module = settings.module("crosshair");
        int s = Pixels.scale(game);
        int length = Math.max(1, module.slider("length").asInt()) * s;
        int gap = module.slider("gap").asInt() * s;
        boolean outline = module.bool("outline").get();
        int color = module.bool("accent").get() ? Colors.withAlpha(accent, 0xFF) : 0xFFFFFFFF;

        int screenWidth = game.screenWidth() * s;
        int screenHeight = game.screenHeight() * s;

        int thickX = Math.max(1, module.slider("thickness").asInt() * s);
        int thickY = thickX;
        if (((screenWidth - thickX) & 1) != 0) {
            thickX++;
        }
        if (((screenHeight - thickY) & 1) != 0) {
            thickY++;
        }

        int left = (screenWidth - thickX) / 2;
        int top = (screenHeight - thickY) / 2;

        game.pushScale(1f / s);
        arm(game, left - gap - length, top, length, thickY, color, outline, s);
        arm(game, left + thickX + gap, top, length, thickY, color, outline, s);
        arm(game, left, top - gap - length, thickX, length, color, outline, s);
        arm(game, left, top + thickY + gap, thickX, length, color, outline, s);
        if (module.bool("dot").get()) {
            arm(game, left, top, thickX, thickY, color, outline, s);
        }
        game.popScale();
    }

    private static void arm(GameBridge game, int x, int y, int width, int height, int color, boolean outline, int scale) {
        if (width <= 0 || height <= 0) {
            return;
        }
        if (outline) {
            int edge = Math.max(1, scale / 2);
            game.fill(x - edge, y - edge, width + edge * 2, height + edge * 2, 0x9E000000);
        }
        game.fill(x, y, width, height, color);
    }

    // ------------------------------------------------------------------ счётчики

    private void trackClicks(GameBridge game, long now, boolean menuOpen) {
        for (int button = 0; button < 2; button++) {
            boolean down = game.mouseDown(button);
            if (down && !mouseWas[button] && !menuOpen) {
                clicks[button][clickHead[button]] = now;
                clickHead[button] = (clickHead[button] + 1) % clicks[button].length;
                if (clickFilled[button] < clicks[button].length) {
                    clickFilled[button]++;
                }
            }
            mouseWas[button] = down;
        }
    }

    private static int gradeFps(int value) {
        if (value >= 120) {
            return Theme.GOOD;
        }
        if (value >= 45) {
            return Theme.TEXT;
        }
        return value >= 25 ? Theme.WARN : Theme.DANGER;
    }

    private static int gradePing(int value) {
        if (value < 0) {
            return Theme.TEXT_MUTED;
        }
        if (value <= 60) {
            return Theme.GOOD;
        }
        return value <= 140 ? Theme.WARN : Theme.DANGER;
    }

    private static String coordinates(HudSnapshot data, Module module) {
        if (data == null || !data.inWorld()) {
            return "вне мира";
        }
        double scale = module.bool("nether").get() ? 0.125 : 1.0;
        int x = (int) Math.floor(data.playerX() * scale);
        int y = (int) Math.floor(data.playerY());
        int z = (int) Math.floor(data.playerZ() * scale);
        String text = x + " " + y + " " + z;
        if (module.bool("direction").get()) {
            text = text + "  " + facing(data.playerYaw());
        }
        return text;
    }

    /**
     * Поворот камеры бывает любым числом, в том числе отрицательным и больше круга,
     * поэтому его сначала приводим к одному обороту.
     */
    private static String facing(float yaw) {
        float normalized = ((yaw % 360f) + 360f) % 360f;
        int index = (int) Math.floor((normalized + 22.5f) / 45f) % FACINGS.length;
        return FACINGS[index];
    }

    private static String clock(HudSnapshot data, Module module) {
        if (data == null) {
            return "--:--";
        }
        boolean half = module.choice("format").index() == 1;
        int hour = data.hour();
        String suffix = "";
        if (half) {
            suffix = hour >= 12 ? " PM" : " AM";
            hour = hour % 12;
            if (hour == 0) {
                hour = 12;
            }
        }
        String text = two(hour) + ":" + two(data.minute());
        if (module.bool("seconds").get()) {
            text = text + ":" + two(data.second());
        }
        return text + suffix;
    }

    private static String session(HudSnapshot data) {
        long seconds = data == null ? 0L : data.sessionSeconds();
        long hours = seconds / 3600L;
        long minutes = (seconds % 3600L) / 60L;
        if (hours > 0) {
            return hours + " ч " + minutes + " мин";
        }
        return minutes + " мин " + (seconds % 60L) + " с";
    }

    private static String memory(HudSnapshot data, Module module) {
        long used = data.usedMemory() / (1024L * 1024L);
        long max = data.maxMemory() / (1024L * 1024L);
        if (!module.bool("percent").get() || max <= 0L) {
            return used + " / " + max + " MB";
        }
        int percent = (int) Math.min(100L, used * 100L / max);
        return used + " MB · " + percent + "%";
    }

    private static String formatFrameTime(float milliseconds) {
        return String.format(java.util.Locale.ROOT, "%.1f ms", milliseconds);
    }

    private static String two(int value) {
        return value < 10 ? "0" + value : Integer.toString(value);
    }
}
