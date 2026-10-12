package dev.nimbus.core.hud;

import dev.nimbus.core.ClientFiles;

import java.io.IOException;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import java.util.Properties;

/**
 * Раскладка панелей на экране.
 *
 * Положение хранится не в пикселях, а долей свободного места от 0 до 1.
 * Панель, прижатая к правому краю в окне, останется у правого края и в
 * полного экрана, и при другом масштабе интерфейса. С абсолютными координатами
 * любая смена разрешения выкидывала бы половину панелей за край.
 *
 * Файл отдельный от настроек: раскладку удобно сбросить целиком, не теряя всё остальное.
 *
 * Испорченная строка стоит одной панели, а не всей раскладки: панель без записи
 * просто встаёт на место по умолчанию. Ронять из-за одной строки весь труд игрока
 * по расстановке двух десятков плашек - несоразмерно.
 */
public final class HudLayout {

    /** Имя файла раскладки. Рядом с настройками, но отдельно от них. */
    private static final String FILE_NAME = "hud.properties";

    /**
     * Предел числа записей.
     *
     * Панелей у клиента десятки, а не тысячи. Чужой или разросшийся файл не должен
     * превращаться в бесконечную карту, которую потом каждый кадр обходит отрисовка.
     */
    private static final int MAX_ENTRIES = 256;

    private final Path file;
    private final Map<String, float[]> positions = new HashMap<>();
    private boolean dirty;
    private boolean loaded;
    private boolean saveFailed;

    public HudLayout() {
        this(ClientFiles.resolve(FILE_NAME));
    }

    /** Явный путь: нужен офлайн-проверке, у которой нет каталога игрока. */
    public HudLayout(Path file) {
        this.file = file;
    }

    public void load() {
        if (loaded) {
            return;
        }
        loaded = true;
        Properties properties;
        try {
            properties = ClientFiles.read(file);
        } catch (IOException | RuntimeException error) {
            // Файл есть, но не читается: играем с раскладкой по умолчанию и говорим об этом.
            saveFailed = true;
            return;
        }
        if (properties == null) {
            return;
        }
        for (String name : properties.stringPropertyNames()) {
            if (positions.size() >= MAX_ENTRIES) {
                break;
            }
            float[] point = parsePoint(properties.getProperty(name));
            if (point != null) {
                positions.put(name, point);
            }
        }
    }

    /**
     * Разбор строки "x,y".
     *
     * @return точку или null, если строка не пара чисел в пределах экрана
     */
    private static float[] parsePoint(String raw) {
        int comma = raw == null ? -1 : raw.indexOf(',');
        if (comma <= 0) {
            return null;
        }
        try {
            float x = Float.parseFloat(raw.substring(0, comma).trim());
            float y = Float.parseFloat(raw.substring(comma + 1).trim());
            // Не-число здесь значит "положения нет": ноль на его месте утащил бы
            // панель в левый верхний угол вместо места по умолчанию.
            if (!Float.isFinite(x) || !Float.isFinite(y)) {
                return null;
            }
            return new float[]{clamp(x), clamp(y)};
        } catch (NumberFormatException malformed) {
            return null;
        }
    }

    public boolean has(String key) {
        return positions.containsKey(key);
    }

    public boolean dirty() {
        return dirty;
    }

    public boolean saveFailed() {
        return saveFailed;
    }

    /** Сколько панелей переставлено игроком. Нужно офлайн-проверке и редактору. */
    public int size() {
        return positions.size();
    }

    public float[] get(String key) {
        return positions.get(key);
    }

    public void set(String key, float x, float y) {
        // Не-число приходит только из сломанного расчёта перетаскивания. Записать
        // его значит сохранить панель в углу навсегда, поэтому такое просто не берём.
        if (key == null || !Float.isFinite(x) || !Float.isFinite(y)) {
            return;
        }
        float[] current = positions.get(key);
        float clampedX = clamp(x);
        float clampedY = clamp(y);
        if (current != null && Math.abs(current[0] - clampedX) < 0.0005f && Math.abs(current[1] - clampedY) < 0.0005f) {
            return;
        }
        if (current == null && positions.size() >= MAX_ENTRIES) {
            return;
        }
        positions.put(key, new float[]{clampedX, clampedY});
        dirty = true;
    }

    /** Сбросить одну панель к положению по умолчанию. */
    public void reset(String key) {
        if (positions.remove(key) != null) {
            dirty = true;
        }
    }

    public void resetAll() {
        if (!positions.isEmpty()) {
            positions.clear();
            dirty = true;
        }
    }

    public boolean saveIfDirty() {
        if (!dirty) {
            return true;
        }
        Properties properties = new Properties();
        for (Map.Entry<String, float[]> entry : positions.entrySet()) {
            float[] value = entry.getValue();
            properties.setProperty(entry.getKey(), round(value[0]) + "," + round(value[1]));
        }
        try {
            ClientFiles.writeAtomic(file, properties, "Nimbus HUD layout");
        } catch (IOException | RuntimeException error) {
            // Раскладка осталась только в памяти: пометку видит меню и говорит игроку.
            saveFailed = true;
            return false;
        }
        dirty = false;
        saveFailed = false;
        return true;
    }

    /**
     * Число с четырьмя знаками после точки без обращения к форматированию.
     *
     * Локаль в игре бывает любой, а Float.toString и String.format в некоторых из
     * них дают запятую - ровно тот символ, которым разделены две координаты.
     */
    private static String round(float value) {
        long units = Math.round(value * 10000f);
        return units / 10000L + "." + pad(units % 10000L);
    }

    private static String pad(long value) {
        String text = Long.toString(Math.abs(value));
        while (text.length() < 4) {
            text = "0" + text;
        }
        return text;
    }

    private static float clamp(float value) {
        if (value < 0f) {
            return 0f;
        }
        return value > 1f ? 1f : value;
    }
}
