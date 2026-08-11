package dev.nimbus.bridge;

/**
 * Перевод читаемых имён игры в те, что реально есть в запущенной версии.
 *
 * Таблица имён живёт в рантайме, но нужна адаптерам. Чтобы адаптеры не зависели
 * от рантайма, между ними стоит этот узкий интерфейс.
 *
 * Все методы возвращают null, если имя неизвестно: вызывающий сам решает,
 * отказаться от функции или подставить читаемое имя (деобфусцированные сборки).
 */
public interface Mappings {

    /** Читаемое имя класса в бинарное имя запущенной версии (a.b.c). */
    String className(String deobfName);

    /**
     * Имя метода в запущенной версии.
     *
     * @param deobfClass читаемое имя класса, например net.minecraft.client.Minecraft
     * @param name       читаемое имя метода, например getWindow
     * @param argTypes   читаемые типы аргументов, например "int", "java.lang.String"
     */
    String methodName(String deobfClass, String name, String... argTypes);

    /** Имя поля в запущенной версии. */
    String fieldName(String deobfClass, String name);

    /** Пустая таблица: все имена возвращаются как есть. */
    Mappings IDENTITY = new Mappings() {

        @Override
        public String className(String deobfName) {
            return deobfName;
        }

        @Override
        public String methodName(String deobfClass, String name, String... argTypes) {
            return name;
        }

        @Override
        public String fieldName(String deobfClass, String name) {
            return name;
        }
    };
}
