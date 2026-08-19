package dev.nimbus.core.config;

import dev.nimbus.bridge.Keys;
import dev.nimbus.core.render.Colors;

/**
 * Одна настройка клиента.
 *
 * Интерфейс ничего не знает про конкретные функции: он рисует то, что описано
 * здесь. Поэтому новая функция стоит одной строки в списке модулей, а не правки
 * отрисовки, сохранения и меню в трёх разных местах.
 *
 * Сохранение тоже живёт здесь: каждая настройка умеет превращаться в строку и
 * читаться обратно, и хранилище не знает про типы вообще.
 */
public abstract class Option {

    /** Кому сообщить, что значение изменилось и файл настроек устарел. */
    public interface Listener {
        void changed();
    }

    private final String key;
    private final String title;
    private final String hint;
    private Listener listener;

    protected Option(String key, String title, String hint) {
        this.key = key;
        this.title = title;
        this.hint = hint == null ? "" : hint;
    }

    public final String key() {
        return key;
    }

    public final String title() {
        return title;
    }

    public final String hint() {
        return hint;
    }

    public final void listen(Listener value) {
        this.listener = value;
    }

    protected final void changed() {
        if (listener != null) {
            listener.changed();
        }
    }

    /** Значение строкой для интерфейса. */
    public abstract String display();

    public abstract String serialize();

    public abstract void deserialize(String raw);

    /**
     * Вернуть значение по умолчанию.
     *
     * Значение по умолчанию - это то, с которым настройка была создана, а не то,
     * что лежит в файле: файл читается позже через deserialize и эталон не трогает.
     * Сброс идёт через обычный set, поэтому слушатель узнает об изменении и файл
     * настроек будет перезаписан.
     */
    public abstract void reset();

    /** Переключатель. */
    public static final class Bool extends Option {

        private final boolean defaultValue;
        private boolean value;

        public Bool(String key, String title, String hint, boolean value) {
            super(key, title, hint);
            this.value = value;
            this.defaultValue = value;
        }

        public boolean get() {
            return value;
        }

        public void set(boolean next) {
            if (next != value) {
                value = next;
                changed();
            }
        }

        public void toggle() {
            set(!value);
        }

        @Override
        public void reset() {
            set(defaultValue);
        }

        @Override
        public String display() {
            return value ? "вкл" : "выкл";
        }

        @Override
        public String serialize() {
            return Boolean.toString(value);
        }

        @Override
        public void deserialize(String raw) {
            value = Boolean.parseBoolean(raw.trim());
        }
    }

    /** Число в диапазоне: ползунок. */
    public static final class Slider extends Option {

        private final float min;
        private final float max;
        private final float step;
        private final String suffix;
        private final float defaultValue;
        private float value;

        public Slider(String key, String title, String hint, float value, float min, float max, float step, String suffix) {
            super(key, title, hint);
            this.min = min;
            this.max = max;
            this.step = step <= 0f ? 1f : step;
            this.suffix = suffix == null ? "" : suffix;
            this.value = snap(value);
            this.defaultValue = this.value;
        }

        public float get() {
            return value;
        }

        public int asInt() {
            return Math.round(value);
        }

        public void set(float next) {
            float snapped = snap(next);
            if (Math.abs(snapped - value) > 0.0001f) {
                value = snapped;
                changed();
            }
        }

        public float fraction() {
            return max - min <= 0f ? 0f : (value - min) / (max - min);
        }

        public void setFraction(float fraction) {
            set(min + (max - min) * clamp01(fraction));
        }

        @Override
        public void reset() {
            set(defaultValue);
        }

        @Override
        public String display() {
            String text;
            if (step >= 1f) {
                text = Integer.toString(Math.round(value));
            } else {
                text = Float.toString(Math.round(value * 100f) / 100f);
            }
            return suffix.isEmpty() ? text : text + suffix;
        }

        @Override
        public String serialize() {
            return Float.toString(value);
        }

        @Override
        public void deserialize(String raw) {
            try {
                value = snap(Float.parseFloat(raw.trim()));
            } catch (NumberFormatException ignored) {
                // Битое значение в файле не повод терять все настройки.
            }
        }

        private float snap(float raw) {
            float clamped = Math.max(min, Math.min(max, raw));
            float steps = Math.round((clamped - min) / step);
            return Math.max(min, Math.min(max, min + steps * step));
        }

        private static float clamp01(float raw) {
            if (raw < 0f) {
                return 0f;
            }
            return raw > 1f ? 1f : raw;
        }
    }

    /** Выбор из нескольких вариантов. */
    public static final class Choice extends Option {

        private final String[] titles;
        private final int defaultIndex;
        private int index;

        public Choice(String key, String title, String hint, int index, String... titles) {
            super(key, title, hint);
            this.titles = titles;
            this.index = clamp(index);
            this.defaultIndex = this.index;
        }

        public int index() {
            return index;
        }

        public int count() {
            return titles.length;
        }

        public String titleAt(int at) {
            return titles[clamp(at)];
        }

        public void set(int next) {
            int clamped = clamp(next);
            if (clamped != index) {
                index = clamped;
                changed();
            }
        }

        public void next() {
            set((index + 1) % titles.length);
        }

        public void previous() {
            set((index - 1 + titles.length) % titles.length);
        }

        @Override
        public void reset() {
            set(defaultIndex);
        }

        @Override
        public String display() {
            return titles[index];
        }

        @Override
        public String serialize() {
            return Integer.toString(index);
        }

        @Override
        public void deserialize(String raw) {
            try {
                index = clamp(Integer.parseInt(raw.trim()));
            } catch (NumberFormatException ignored) {
                // Пусть останется значение по умолчанию.
            }
        }

        private int clamp(int raw) {
            if (raw < 0) {
                return 0;
            }
            return raw >= titles.length ? titles.length - 1 : raw;
        }
    }

    /** Назначенная клавиша. */
    public static final class Key extends Option {

        private final int defaultCode;
        private int code;

        public Key(String key, String title, String hint, int code) {
            super(key, title, hint);
            this.code = code;
            this.defaultCode = code;
        }

        public int code() {
            return code;
        }

        public void set(int next) {
            if (next != code) {
                code = next;
                changed();
            }
        }

        @Override
        public void reset() {
            set(defaultCode);
        }

        @Override
        public String display() {
            return Keys.name(code);
        }

        @Override
        public String serialize() {
            return Integer.toString(code);
        }

        @Override
        public void deserialize(String raw) {
            try {
                code = Integer.parseInt(raw.trim());
            } catch (NumberFormatException ignored) {
                // Остаётся клавиша по умолчанию.
            }
        }
    }

    /**
     * Акцентный цвет.
     *
     * Хранится тон и насыщенность, а не готовое число: из тона можно честно
     * построить и светлую, и тёмную версию цвета для градиентов и свечения, а из
     * готового числа - только угадать.
     */
    public static final class Color extends Option {

        private final float defaultHue;
        private final float defaultSaturation;
        private float hue;
        private float saturation;

        public Color(String key, String title, String hint, float hue, float saturation) {
            super(key, title, hint);
            this.hue = wrap(hue);
            this.saturation = clamp01(saturation);
            this.defaultHue = this.hue;
            this.defaultSaturation = this.saturation;
        }

        public float hue() {
            return hue;
        }

        public float saturation() {
            return saturation;
        }

        public void setHue(float next) {
            float wrapped = wrap(next);
            if (Math.abs(wrapped - hue) > 0.01f) {
                hue = wrapped;
                changed();
            }
        }

        public void setSaturation(float next) {
            float clamped = clamp01(next);
            if (Math.abs(clamped - saturation) > 0.001f) {
                saturation = clamped;
                changed();
            }
        }

        public void set(float nextHue, float nextSaturation) {
            setHue(nextHue);
            setSaturation(nextSaturation);
        }

        @Override
        public void reset() {
            set(defaultHue, defaultSaturation);
        }

        /** Основной акцент. */
        public int argb() {
            return Colors.hsv(hue, saturation, 1f, 0xFF);
        }

        /** Светлый край градиента. */
        public int light() {
            return Colors.hsv(hue - 12f, Math.max(0f, saturation - 0.18f), 1f, 0xFF);
        }

        /** Тёмный край градиента. */
        public int deep() {
            return Colors.hsv(hue + 16f, Math.min(1f, saturation + 0.12f), 0.82f, 0xFF);
        }

        @Override
        public String display() {
            int color = argb();
            return "#" + hex(Colors.red(color)) + hex(Colors.green(color)) + hex(Colors.blue(color));
        }

        @Override
        public String serialize() {
            return hue + ";" + saturation;
        }

        @Override
        public void deserialize(String raw) {
            String[] parts = raw.split(";");
            if (parts.length != 2) {
                return;
            }
            try {
                hue = wrap(Float.parseFloat(parts[0].trim()));
                saturation = clamp01(Float.parseFloat(parts[1].trim()));
            } catch (NumberFormatException ignored) {
                // Цвет по умолчанию лучше, чем падение загрузки.
            }
        }

        private static String hex(int value) {
            String text = Integer.toHexString(value).toUpperCase();
            return text.length() < 2 ? "0" + text : text;
        }

        private static float wrap(float raw) {
            return ((raw % 360f) + 360f) % 360f;
        }

        private static float clamp01(float raw) {
            if (raw < 0f) {
                return 0f;
            }
            return raw > 1f ? 1f : raw;
        }
    }
}
