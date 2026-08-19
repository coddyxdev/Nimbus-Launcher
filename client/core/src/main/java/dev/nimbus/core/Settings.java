package dev.nimbus.core;

import dev.nimbus.bridge.Keys;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.config.Option;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
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

    private final List<Module> modules = new ArrayList<>();
    private final Map<String, Module> index = new LinkedHashMap<>();

    private final Module hud;
    private final Module keys;
    private final Module theme;

    private boolean dirty;

    public Settings() {
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
                new Option.Bool("version", "Версия игры", "Показывать рядом с названием", true)
        ));
        register(Module.of(
                "fps",
                "Счётчик кадров",
                "Сколько кадров в секунду выдаёт игра",
                Module.Category.HUD,
                true,
                new Option.Bool("colorize", "Цвет по нагрузке", "Зелёный, жёлтый, красный", true)
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
                "История FPS за последние полминуты",
                Module.Category.VISUAL,
                false,
                new Option.Slider("width", "Ширина", "Размер окна графика", 110f, 60f, 220f, 10f, ""),
                new Option.Slider("height", "Высота", "Размер окна графика", 34f, 20f, 70f, 2f, ""),
                new Option.Bool("fill", "Заливка", "Заливать площадь под линией", true)
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
    public void saveIfDirty() {
        if (!dirty) {
            return;
        }
        dirty = false;
        save();
    }

    public boolean dirty() {
        return dirty;
    }

    public void load() {
        Path file = file();
        if (file == null || !Files.isRegularFile(file)) {
            return;
        }
        Properties props = new Properties();
        try (InputStream in = Files.newInputStream(file)) {
            props.load(in);
        } catch (IOException | RuntimeException error) {
            return;
        }
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
    }

    public void save() {
        Path file = file();
        if (file == null) {
            return;
        }
        Properties props = new Properties();
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
            Files.createDirectories(file.getParent());
            try (OutputStream out = Files.newOutputStream(file)) {
                props.store(out, "Nimbus Client");
            }
        } catch (IOException | RuntimeException ignored) {
            // Настройки не стоят падения игры.
        }
    }

    private static Path file() {
        try {
            String appData = System.getenv("APPDATA");
            Path root = appData != null && !appData.isEmpty()
                    ? Paths.get(appData, "NimbusClient")
                    : Paths.get(System.getProperty("user.home", "."), ".nimbusclient");
            return root.resolve("client").resolve("settings.properties");
        } catch (RuntimeException error) {
            return null;
        }
    }
}
