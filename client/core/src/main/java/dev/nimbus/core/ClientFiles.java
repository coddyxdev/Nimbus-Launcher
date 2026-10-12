package dev.nimbus.core;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.util.Properties;

/**
 * Файлы состояния клиента: где лежат и как переживают падение игры.
 *
 * Раньше путь и запись через временный файл были написаны дважды - в настройках
 * и в раскладке HUD. Две копии одного правила расходятся молча: одна начинает
 * писать в другой каталог или теряет откат при сбое, и понять это можно только
 * по пропавшим настройкам игрока. Правило живёт здесь в одном экземпляре.
 *
 * Запись всегда идёт во временный файл рядом с целевым, и только потом файл
 * подменяется целиком. Игру закрывают крестиком в любой момент кадра, и
 * недописанный файл настроек означает потерю всего, что игрок настроил.
 */
public final class ClientFiles {

    /**
     * Предел размера файла состояния.
     *
     * Свои файлы - это единицы килобайт. Всё, что больше мегабайта, либо не наше,
     * либо испорчено: читать это в память в игровом потоке незачем.
     */
    public static final long MAX_STATE_BYTES = 1L << 20;

    private ClientFiles() {
    }

    /**
     * Путь к файлу состояния клиента по имени.
     *
     * @param name имя файла, например settings.properties
     * @return путь или null, если домашний каталог недоступен
     */
    public static Path resolve(String name) {
        try {
            String appData = System.getenv("APPDATA");
            Path root = appData != null && !appData.isEmpty()
                    ? Paths.get(appData, "NimbusClient")
                    : Paths.get(System.getProperty("user.home", "."), ".nimbusclient");
            return root.resolve("client").resolve(name);
        } catch (RuntimeException error) {
            return null;
        }
    }

    /**
     * Прочитать файл состояния.
     *
     * @return разобранные значения или null, если файла нет
     * @throws IOException файл есть, но прочитать его не удалось: сюда же сведены
     *                    и разборные ошибки формата, чтобы у вызывающего был один
     *                    путь обработки отказа вместо двух
     */
    public static Properties read(Path file) throws IOException {
        if (file == null || !Files.isRegularFile(file)) {
            return null;
        }
        long size = Files.size(file);
        if (size > MAX_STATE_BYTES) {
            throw new IOException("файл состояния слишком велик: " + size + " байт");
        }
        Properties properties = new Properties();
        try (InputStream input = Files.newInputStream(file)) {
            properties.load(input);
        } catch (RuntimeException error) {
            // Properties бросает IllegalArgumentException на испорченном экранировании.
            throw new IOException("файл состояния не разобран", error);
        }
        return properties;
    }

    /**
     * Записать файл состояния целиком и неделимо.
     *
     * @throws IOException записать не удалось; временный файл при этом удаляется,
     *                     а прежнее содержимое целевого остаётся нетронутым
     */
    public static void writeAtomic(Path file, Properties properties, String comment) throws IOException {
        if (file == null || file.getFileName() == null) {
            throw new IOException("путь файла состояния неизвестен");
        }
        Path parent = file.getParent();
        if (parent == null) {
            throw new IOException("у файла состояния нет каталога: " + file);
        }
        Path temporary = file.resolveSibling(file.getFileName() + ".tmp");
        try {
            Files.createDirectories(parent);
            try (OutputStream output = Files.newOutputStream(temporary)) {
                properties.store(output, comment);
            }
            move(temporary, file);
        } catch (IOException error) {
            deleteQuietly(temporary);
            throw error;
        } catch (RuntimeException error) {
            deleteQuietly(temporary);
            throw new IOException("запись файла состояния не удалась", error);
        }
    }

    /**
     * Подмена файла: сначала неделимо, потом как получится.
     *
     * Неделимая подмена умеет отказать не только на файловой системе, которая её
     * не поддерживает: на Windows целевой файл в этот момент может быть открыт
     * антивирусом или другой копией игры. Тогда остаётся обычная замена - она
     * менее надёжна, но лучше, чем потерянные настройки.
     */
    private static void move(Path temporary, Path target) throws IOException {
        try {
            Files.move(temporary, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException retryable) {
            Files.move(temporary, target, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static void deleteQuietly(Path file) {
        try {
            Files.deleteIfExists(file);
        } catch (IOException | RuntimeException ignored) {
            // Мусорный временный файл не повод скрывать настоящую ошибку записи.
        }
    }
}
