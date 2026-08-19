package dev.nimbus.core.config;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Модуль клиента: одна функция со своими настройками.
 *
 * Сознательно есть два вида: переключаемый модуль (его можно выключить целиком)
 * и группа настроек (она всегда активна - выключить "оформление" нельзя).
 * Интерфейс рисует их по-разному, а не показывает мёртвый переключатель.
 */
public final class Module {

    /** Раздел меню. */
    public enum Category {

        HUD("Интерфейс", "Информация поверх игры"),
        VISUAL("Визуал", "Прицел, клавиши, графики"),
        CONTROL("Управление", "Клавиши и звук клиента"),
        THEME("Оформление", "Цвет, фон и плотность");

        private final String title;
        private final String hint;

        Category(String title, String hint) {
            this.title = title;
            this.hint = hint;
        }

        public String title() {
            return title;
        }

        public String hint() {
            return hint;
        }
    }

    private final String key;
    private final String title;
    private final String description;
    private final Category category;
    private final boolean toggleable;
    private final Option.Bool enabled;
    private final List<Option> options;

    private Module(
            String key,
            String title,
            String description,
            Category category,
            boolean toggleable,
            boolean defaultOn,
            Option[] options
    ) {
        this.key = key;
        this.title = title;
        this.description = description;
        this.category = category;
        this.toggleable = toggleable;
        this.enabled = new Option.Bool("enabled", title, description, defaultOn);
        List<Option> list = new ArrayList<>(options.length);
        Collections.addAll(list, options);
        this.options = Collections.unmodifiableList(list);
    }

    /** Модуль с переключателем. */
    public static Module of(
            String key,
            String title,
            String description,
            Category category,
            boolean defaultOn,
            Option... options
    ) {
        return new Module(key, title, description, category, true, defaultOn, options);
    }

    /** Группа настроек без переключателя. */
    public static Module group(
            String key,
            String title,
            String description,
            Category category,
            Option... options
    ) {
        return new Module(key, title, description, category, false, true, options);
    }

    public String key() {
        return key;
    }

    public String title() {
        return title;
    }

    public String description() {
        return description;
    }

    public Category category() {
        return category;
    }

    public boolean toggleable() {
        return toggleable;
    }

    public Option.Bool enabledOption() {
        return enabled;
    }

    public List<Option> options() {
        return options;
    }

    public boolean on() {
        return !toggleable || enabled.get();
    }

    public void setOn(boolean value) {
        if (toggleable) {
            enabled.set(value);
        }
    }

    public void toggle() {
        setOn(!enabled.get());
    }

    /**
     * Вернуть модуль к заводским настройкам.
     *
     * Переключатель сбрасывается вместе с настройками, иначе "сброс" оставлял бы
     * половину состояния прежней. У группы настроек переключателя нет, но сброс
     * её собственного флага безвреден: on() у неё всё равно всегда true.
     */
    public void resetAll() {
        enabled.reset();
        for (int i = 0; i < options.size(); i++) {
            options.get(i).reset();
        }
    }

    public Option option(String optionKey) {
        for (int i = 0; i < options.size(); i++) {
            Option option = options.get(i);
            if (option.key().equals(optionKey)) {
                return option;
            }
        }
        throw new IllegalArgumentException("нет настройки " + key + "." + optionKey);
    }

    public Option.Bool bool(String optionKey) {
        return (Option.Bool) option(optionKey);
    }

    public Option.Slider slider(String optionKey) {
        return (Option.Slider) option(optionKey);
    }

    public Option.Choice choice(String optionKey) {
        return (Option.Choice) option(optionKey);
    }

    public Option.Key keyOption(String optionKey) {
        return (Option.Key) option(optionKey);
    }

    public Option.Color color(String optionKey) {
        return (Option.Color) option(optionKey);
    }
}
