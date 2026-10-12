package dev.nimbus.core;

import dev.nimbus.bridge.Keys;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.config.Option;

import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;

/**
 * Все модули клиента и их хранение на диске.
 *
 * Список ниже - единственное место, где описано, что умеет клиент. Меню строится
 * по нему само, сохранение тоже идёт по нему: добавленная строка сразу появляется в
 * интерфейсе и сразу переживает перезапуск игры.
 *
 * Формат файла нарочно самый простой из возможных: ключ-значение из стандартной
 * библиотеки. Клиент живёт внутри чужого процесса, и тащить сюда библиотеку
 * разбора JSON ради настроек - лишний риск столкновения версий с модами игры.
 */
public final class Settings {

    /** Угол экрана для стопки HUD. */
    public static final String[] CORNERS = {
            "Слева сверху",
            "Справа сверху",
            "Слева снизу",
            "Справа снизу"
    };

    /** Что показывает быстрое колесо по удержанию клавиши. */
    private static final String[] WHEEL = {"fps", "cps", "coords", "clock", "keystrokes", "crosshair"};

    private static final int DEFAULT_GUI_CATEGORY = 0;
    private static final float DEFAULT_GUI_WINDOW_X = 0.5f;
    private static final float DEFAULT_GUI_WINDOW_Y = 0.5f;

    private final List<Module> modules = new ArrayList<>();
    private final Map<String, Module> index = new LinkedHashMap<>();
    private final Path settingsFile;

    private final Module hud;
    private final Module keys;
    private final Module theme;

    private boolean dirty;
    private boolean saveFailed;
    private int guiCategory = DEFAULT_GUI_CATEGORY;
    private float guiWindowX = DEFAULT_GUI_WINDOW_X;
    private float guiWindowY = DEFAULT_GUI_WINDOW_Y;

    public Settings() {
        this(file());
    }

    public Settings(Path settingsFile) {
        this.settingsFile = settingsFile;
        hud = Module.group(
                "hud",
                "Стопка HUD",
                "Где и как лежат информационные плашки",
                Module.Category.HUD,
                new Option.Choice("corner", "Угол экрана", "Откуда растёт стопка", 0, CORNERS),
                new Option.Choice("layout", "Раскладка", "Столбцом или строкой", 0, "Столбец", "Строка"),
                new Option.Slider("opacity", "Плотность фона", "Насколько плашки непрозрачны", 85f, 0f, 100f, 5f, "%"),
                new Option.Bool("accentBar", "Акцентная риска", "Цветная полоска слева у плашки", true),
                new Option.Bool("labels", "Подписи", "Показывать название значения", true)
        );

        keys = Module.group(
                "keys",
                "Клавиши и звук",
                "Как открывать меню и колесо",
                Module.Category.CONTROL,
                new Option.Key("menu", "Открыть меню", "Полное окно настроек", Keys.RIGHT_SHIFT),
                new Option.Key("wheel", "Быстрое колесо", "Удержание и жест мышью", Keys.RIGHT_CONTROL),
                new Option.Bool("sound", "Звук интерфейса", "Щелчки и тики при выборе", true),
                new Option.Slider("speed", "Скорость анимаций", "Больше - резче", 1f, 0.5f, 2f, 0.1f, "x")
        );

        theme = Module.group(
                "theme",
                "Оформление",
                "Цвет клиента и фон меню",
                Module.Category.THEME,
                new Option.Color("accent", "Акцент", "Главный цвет интерфейса", 222f, 0.68f),
                new Option.Choice("surface", "Фон окна", "Оттенок тёмных поверхностей", 0, dev.nimbus.core.ui.Theme.SURFACE_NAMES),
                new Option.Slider("dim", "Затемнение игры", "При открытом меню", 45f, 0f, 85f, 5f, "%"),
                new Option.Bool("glow", "Свечение акцента", "Мягкий ореол у активных элементов", true)
        );

        register(Module.of(
                "watermark",
                "Логотип клиента",
                "Подпись Nimbus и версия игры",
                Module.Category.HUD,
                true,
                new Option.Choice("style", "Стиль", "Как выглядит логотип", 0, "Классика", "Минимал", "Значок"),
                new Option.Text("label", "Своя подпись", "Пусто - обычное NIMBUS", ""),
                new Option.Bool("version", "Версия игры", "Показывать рядом с названием", true)
        ));
        register(Module.of(
                "fps",
                "Счётчик кадров",
                "Сколько кадров в секунду выдаёт игра",
                Module.Category.HUD,
                true,
                new Option.Bool("colorize", "Цвет по нагрузке", "Зелёный, жёлтый, красный", true),
                new Option.Bool("low", "1% low", "Самые медленные кадры за последнее окно", true),
                new Option.Bool("frameTime", "Время кадра", "Показывать миллисекунды рядом с FPS", false)
        ));
        register(Module.of(
                "memory",
                "Память JVM",
                "Локальная диагностика выделенной памяти",
                Module.Category.HUD,
                false,
                new Option.Bool("percent", "Процент", "Показывать долю занятой памяти", true)
        ));
        register(Module.of(
                "cps",
                "Счётчик кликов",
                "Клики в секунду по кнопкам мыши",
                Module.Category.HUD,
                false,
                new Option.Choice("mode", "Кнопки", "За чем следить", 2, "ЛКМ", "ПКМ", "Обе")
        ));
        register(Module.of(
                "coords",
                "Координаты",
                "Положение игрока в мире",
                Module.Category.HUD,
                false,
                new Option.Bool("direction", "Сторона света", "Куда смотрит камера", true),
                new Option.Bool("nether", "Координаты Ада", "Пересчёт один к восьми", false)
        ));
        register(Module.of(
                "clock",
                "Часы",
                "Реальное время на экране",
                Module.Category.HUD,
                false,
                new Option.Choice("format", "Формат", "Как показывать время", 0, "24 часа", "12 часов"),
                new Option.Bool("seconds", "Секунды", "Показывать секунды", false)
        ));
        register(Module.of(
                "session",
                "Время в игре",
                "Сколько длится текущая сессия",
                Module.Category.HUD,
                false
        ));
        register(Module.of(
                "ping",
                "Пинг",
                "Задержка до сервера",
                Module.Category.HUD,
                false,
                new Option.Bool("colorize", "Цвет по задержке", "Зелёный, жёлтый, красный", true)
        ));
        register(hud);

        register(Module.of(
                "keystrokes",
                "Раскладка клавиш",
                "Живые WASD и кнопки мыши на экране",
                Module.Category.VISUAL,
                false,
                new Option.Bool("mouse", "Кнопки мыши", "ЛКМ и ПКМ отдельным рядом", true),
                new Option.Bool("cps", "CPS на кнопках", "Число кликов внутри клавиши", true),
                new Option.Bool("space", "Пробел", "Полоса прыжка снизу", true),
                new Option.Slider("x", "Положение по горизонтали", "От левого края экрана", 4f, 0f, 100f, 1f, "%"),
                new Option.Slider("y", "Положение по вертикали", "От верхнего края экрана", 55f, 0f, 100f, 1f, "%")
        ));
        register(Module.of(
                "crosshair",
                "Свой прицел",
                "Прицел клиента вместо ванильного",
                Module.Category.VISUAL,
                false,
                new Option.Slider("length", "Длина луча", "Размер каждой черты", 4f, 1f, 12f, 1f, ""),
                new Option.Slider("gap", "Отступ от центра", "Пустота в середине", 3f, 0f, 10f, 1f, ""),
                new Option.Slider("thickness", "Толщина", "Толщина черты", 1f, 1f, 3f, 1f, ""),
                new Option.Bool("dot", "Точка в центре", "Маленькая точка по центру", false),
                new Option.Bool("outline", "Обводка", "Тёмный контур для читаемости", true),
                new Option.Bool("accent", "Красить акцентом", "Иначе белый", false)
        ));
        register(Module.of(
                "graph",
                "График кадров",
                "История FPS: одна точка в пятую долю секунды, чем шире окно, тем дольше память",
                Module.Category.VISUAL,
                false,
                new Option.Slider("width", "Ширина", "Размер окна графика", 110f, 60f, 220f, 10f, ""),
                new Option.Slider("height", "Высота", "Размер окна графика", 34f, 20f, 70f, 2f, ""),
                new Option.Bool("fill", "Заливка", "Заливать площадь под линией", true)
        ));

        // ---------------------------------------------------------------- бой
        register(Module.of(
                "targethud",
                "Панель цели",
                "Кого ударил последним: имя и полоса здоровья",
                Module.Category.COMBAT,
                false,
                new Option.Slider("hold", "Держать после удара", "Сколько секунд панель ещё видна", 4f, 1f, 8f, 0.5f, " с"),
                new Option.Bool("distance", "Дистанция", "Сколько блоков до цели", true)
        ));
        register(Module.of(
                "attackcd",
                "Кулдаун удара",
                "Готовность замаха в полную силу",
                Module.Category.COMBAT,
                false,
                new Option.Choice("style", "Вид", "Полоса или кольцо у прицела", 0, "Полоса", "Кольцо"),
                new Option.Bool("hideFull", "Прятать в готовности", "Панель исчезает, когда замах готов", true)
        ));
        register(Module.of(
                "hitsound",
                "Звук попадания",
                "Короткий звук при каждом ударе по цели",
                Module.Category.COMBAT,
                false,
                new Option.Choice("sound", "Звук", "Обычный или критический", 0, "Обычный", "Критический"),
                new Option.Slider("pitch", "Высота", "Насколько высоким звучит", 0.5f, 0f, 1f, 0.05f, ""),
                new Option.Bool("vary", "Живая высота", "Слегка варьировать от удара к удару", true)
        ));
        register(Module.of(
                "deathcoords",
                "Точка смерти",
                "Координаты последней смерти под рукой",
                Module.Category.COMBAT,
                false,
                new Option.Bool("chat", "Писать в чат", "Координаты уходят и в игровой чат", true)
        ));

        // ---------------------------------------------------------------- статус
        register(Module.of(
                "armor",
                "Броня",
                "Прочность каждого предмета брони",
                Module.Category.HUD,
                false,
                new Option.Bool("percent", "Проценты", "Число прочности рядом с полосой", true),
                new Option.Bool("warn", "Предупреждать", "Красная полоса у почти сломанного", true)
        ));
        register(Module.of(
                "potions",
                "Эффекты",
                "Активные эффекты зелий и их время",
                Module.Category.HUD,
                false,
                new Option.Bool("seconds", "Секунды", "Сколько осталось каждому эффекту", true),
                new Option.Bool("level", "Уровень", "Римские уровни у эффектов", true)
        ));
        register(Module.of(
                "health",
                "Здоровье",
                "Жизни, поглощение и броня одной плашкой",
                Module.Category.HUD,
                false,
                new Option.Bool("absorption", "Поглощение", "Золотые сердца отдельно", true),
                new Option.Bool("armorPoints", "Броня", "Очки брони рядом", true)
        ));
        register(Module.of(
                "speed",
                "Скорость",
                "Как быстро движется игрок, блоков в секунду",
                Module.Category.HUD,
                false,
                new Option.Choice("units", "Единицы", "В чём измерять", 0, "Блоки/с", "Метры/с")
        ));
        register(Module.of(
                "server",
                "Сервер",
                "Адрес и задержка текущего сервера",
                Module.Category.HUD,
                false,
                new Option.Bool("ping", "Пинг", "Задержка рядом с адресом", true)
        ));
        register(Module.of(
                "streamer",
                "Режим стримера",
                "Прятать личные данные во всех плашках",
                Module.Category.CONTROL,
                false,
                new Option.Bool("coords", "Координаты", "Скрывать позицию", true),
                new Option.Bool("server", "Сервер", "Скрывать адрес", true),
                new Option.Bool("name", "Ник", "Скрывать имя игрока", true)
        ));

        // ---------------------------------------------------------------- управление и визуал
        register(Module.of(
                "zoom",
                "Зум",
                "Плавное приближение камеры на удержание",
                Module.Category.VISUAL,
                true,
                new Option.Key("key", "Клавиша зума", "Удержание приближает камеру", Keys.C),
                new Option.Slider("power", "Сила", "Насколько близко приближать", 0.6f, 0.2f, 0.9f, 0.05f, "")
        ));

        register(keys);
        register(theme);

        Option.Listener listener = new Option.Listener() {
            @Override
            public void changed() {
                dirty = true;
            }
        };
        for (int i = 0; i < modules.size(); i++) {
            Module module = modules.get(i);
            module.enabledOption().listen(listener);
            List<Option> options = module.options();
            for (int j = 0; j < options.size(); j++) {
                options.get(j).listen(listener);
            }
        }
    }

    private void register(Module module) {
        modules.add(module);
        index.put(module.key(), module);
    }

    public List<Module> modules() {
        return Collections.unmodifiableList(modules);
    }

    public Module module(String key) {
        Module module = index.get(key);
        if (module == null) {
            throw new IllegalArgumentException("нет модуля " + key);
        }
        return module;
    }

    /** Тот же поиск, но без исключения: для необязательных модулей. */
    public Module moduleOrNull(String key) {
        return index.get(key);
    }

    public List<Module> byCategory(Module.Category category) {
        List<Module> result = new ArrayList<>();
        for (int i = 0; i < modules.size(); i++) {
            Module module = modules.get(i);
            if (module.category() == category) {
                result.add(module);
            }
        }
        return result;
    }

    /** Сколько модулей раздела включено - цифра в боковом меню. */
    public int activeIn(Module.Category category) {
        int count = 0;
        for (int i = 0; i < modules.size(); i++) {
            Module module = modules.get(i);
            if (module.category() == category && module.toggleable() && module.on()) {
                count++;
            }
        }
        return count;
    }

    /** Модули быстрого колеса. */
    public List<Module> wheel() {
        List<Module> result = new ArrayList<>(WHEEL.length);
        for (int i = 0; i < WHEEL.length; i++) {
            Module module = index.get(WHEEL[i]);
            if (module != null) {
                result.add(module);
            }
        }
        return result;
    }

    public int accent() {
        return theme.color("accent").argb();
    }

    public int accentLight() {
        return theme.color("accent").light();
    }

    public int accentDeep() {
        return theme.color("accent").deep();
    }

    public Option.Color accentOption() {
        return theme.color("accent");
    }

    public int surfaceIndex() {
        return theme.choice("surface").index();
    }

    public int guiCategory() {
        return guiCategory;
    }

    public float guiWindowX() {
        return guiWindowX;
    }

    public float guiWindowY() {
        return guiWindowY;
    }

    public void setGuiCategory(int category) {
        int clamped = Math.max(0, Math.min(Module.Category.values().length - 1, category));
        if (guiCategory != clamped) {
            guiCategory = clamped;
            dirty = true;
        }
    }

    public void setGuiWindowPosition(float x, float y) {
        float clampedX = clampUnit(x, DEFAULT_GUI_WINDOW_X);
        float clampedY = clampUnit(y, DEFAULT_GUI_WINDOW_Y);
        if (Math.abs(guiWindowX - clampedX) < 0.0005f && Math.abs(guiWindowY - clampedY) < 0.0005f) {
            return;
        }
        guiWindowX = clampedX;
        guiWindowY = clampedY;
        dirty = true;
    }

    public boolean saveFailed() {
        return saveFailed;
    }

    public float dim() {
        return theme.slider("dim").get() / 100f;
    }

    public boolean glow() {
        return theme.bool("glow").get();
    }

    public boolean sound() {
        return keys.bool("sound").get();
    }

    public float speed() {
        return keys.slider("speed").get();
    }

    public int menuKey() {
        return keys.keyOption("menu").code();
    }

    public int wheelKey() {
        return keys.keyOption("wheel").code();
    }

    public int hudCorner() {
        return hud.choice("corner").index();
    }

    public boolean hudRow() {
        return hud.choice("layout").index() == 1;
    }

    public float hudOpacity() {
        return hud.slider("opacity").get() / 100f;
    }

    public boolean hudAccentBar() {
        return hud.bool("accentBar").get();
    }

    public boolean hudLabels() {
        return hud.bool("labels").get();
    }

    /** Запись только если что-то менялось: дёргать диск каждый кадр недопустимо. */
    public boolean saveIfDirty() {
        if (!dirty) {
            return true;
        }
        return save();
    }

    public boolean dirty() {
        return dirty;
    }

    public void load() {
        Properties props;
        try {
            props = ClientFiles.read(settingsFile);
        } catch (IOException | RuntimeException error) {
            // Файл есть, но не читается: остаёмся на значениях по умолчанию и
            // помечаем отказ, иначе первое же сохранение затрёт чужой файл молча.
            saveFailed = true;
            return;
        }
        if (props == null) {
            saveFailed = false;
            return;
        }
        guiCategory = Math.max(0, Math.min(Module.Category.values().length - 1,
                parseInt(props.getProperty("gui.category"), DEFAULT_GUI_CATEGORY)));
        guiWindowX = parseUnit(props.getProperty("gui.windowX"), DEFAULT_GUI_WINDOW_X);
        guiWindowY = parseUnit(props.getProperty("gui.windowY"), DEFAULT_GUI_WINDOW_Y);

        for (int i = 0; i < modules.size(); i++) {
            Module module = modules.get(i);
            if (module.toggleable()) {
                String raw = props.getProperty(module.key() + ".enabled");
                if (raw != null) {
                    module.enabledOption().deserialize(raw);
                }
            }
            List<Option> options = module.options();
            for (int j = 0; j < options.size(); j++) {
                Option option = options.get(j);
                String raw = props.getProperty(module.key() + "." + option.key());
                if (raw != null) {
                    option.deserialize(raw);
                }
            }
        }
        dirty = false;
        saveFailed = false;
    }

    public boolean save() {
        Path file = settingsFile;
        if (file == null) {
            saveFailed = true;
            return false;
        }
        Properties props = new Properties();
        props.setProperty("gui.category", Integer.toString(guiCategory));
        props.setProperty("gui.windowX", Float.toString(guiWindowX));
        props.setProperty("gui.windowY", Float.toString(guiWindowY));
        for (int i = 0; i < modules.size(); i++) {
            Module module = modules.get(i);
            if (module.toggleable()) {
                props.setProperty(module.key() + ".enabled", module.enabledOption().serialize());
            }
            List<Option> options = module.options();
            for (int j = 0; j < options.size(); j++) {
                Option option = options.get(j);
                props.setProperty(module.key() + "." + option.key(), option.serialize());
            }
        }
        try {
            ClientFiles.writeAtomic(file, props, "Nimbus Client");
        } catch (IOException | RuntimeException error) {
            saveFailed = true;
            return false;
        }
        dirty = false;
        saveFailed = false;
        return true;
    }

    private static int parseInt(String raw, int fallback) {
        if (raw == null) {
            return fallback;
        }
        try {
            return Integer.parseInt(raw.trim());
        } catch (RuntimeException error) {
            return fallback;
        }
    }

    private static float parseUnit(String raw, float fallback) {
        if (raw == null) {
            return fallback;
        }
        try {
            return clampUnit(Float.parseFloat(raw.trim()), fallback);
        } catch (RuntimeException error) {
            return fallback;
        }
    }

    private static float clampUnit(float value, float fallback) {
        if (Float.isNaN(value) || Float.isInfinite(value)) {
            return fallback;
        }
        return Math.max(0f, Math.min(1f, value));
    }

    private static Path file() {
        return ClientFiles.resolve("settings.properties");
    }
}
