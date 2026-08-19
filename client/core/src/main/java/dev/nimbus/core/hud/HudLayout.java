package dev.nimbus.core.hud;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
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
 */
public final class HudLayout {

    private final Map<String, float[]> positions = new HashMap<>();
    private boolean dirty;
    private boolean loaded;

    public void load() {
        if (loaded) {
            return;
        }
        loaded = true;
        File file = file();
        if (file == null || !file.isFile()) {
            return;
        }
        Properties properties = new Properties();
        InputStream input = null;
        try {
            input = new FileInputStream(file);
            properties.load(input);
            for (String name : properties.stringPropertyNames()) {
                String raw = properties.getProperty(name);
                int comma = raw == null ? -1 : raw.indexOf(',');
                if (comma <= 0) {
                    continue;
                }
                float x = Float.parseFloat(raw.substring(0, comma).trim());
                float y = Float.parseFloat(raw.substring(comma + 1).trim());
                positions.put(name, new float[]{clamp(x), clamp(y)});
            }
        } catch (Throwable error) {
            positions.clear();
        } finally {
            close(input);
        }
    }

    public boolean has(String key) {
        return positions.containsKey(key);
    }

    public float[] get(String key) {
        return positions.get(key);
    }

    public void set(String key, float x, float y) {
        float[] current = positions.get(key);
        float clampedX = clamp(x);
        float clampedY = clamp(y);
        if (current != null && Math.abs(current[0] - clampedX) < 0.0005f && Math.abs(current[1] - clampedY) < 0.0005f) {
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

    public void saveIfDirty() {
        if (!dirty) {
            return;
        }
        dirty = false;
        File file = file();
        if (file == null) {
            return;
        }
        File parent = file.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
            return;
        }
        Properties properties = new Properties();
        for (Map.Entry<String, float[]> entry : positions.entrySet()) {
            float[] value = entry.getValue();
            properties.setProperty(entry.getKey(), round(value[0]) + "," + round(value[1]));
        }
        OutputStream output = null;
        try {
            output = new FileOutputStream(file);
            properties.store(output, "Nimbus HUD layout");
        } catch (Throwable error) {
            // Не смогли сохранить - раскладка просто останется до конца сессии.
        } finally {
            close(output);
        }
    }

    private static String round(float value) {
        return Long.toString(Math.round(value * 10000f) / 10000L) + "." + pad(Math.round(value * 10000f) % 10000L);
    }

    private static String pad(long value) {
        String text = Long.toString(Math.abs(value));
        while (text.length() < 4) {
            text = "0" + text;
        }
        return text;
    }

    private static float clamp(float value) {
        if (Float.isNaN(value) || value < 0f) {
            return 0f;
        }
        return value > 1f ? 1f : value;
    }

    private static File file() {
        try {
            String appData = System.getenv("APPDATA");
            File root;
            if (appData != null && !appData.isEmpty()) {
                root = new File(new File(appData, "NimbusClient"), "client");
            } else {
                root = new File(System.getProperty("user.home", "."), ".nimbusclient");
            }
            return new File(root, "hud.properties");
        } catch (Throwable error) {
            return null;
        }
    }

    private static void close(java.io.Closeable stream) {
        if (stream == null) {
            return;
        }
        try {
            stream.close();
        } catch (Throwable error) {
            // Закрытие файла не повод ломать кадр.
        }
    }
}
