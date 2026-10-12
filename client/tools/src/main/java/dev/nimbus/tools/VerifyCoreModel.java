package dev.nimbus.tools;

import dev.nimbus.core.Settings;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.hud.Hud;
import dev.nimbus.core.hud.HudLayout;
import dev.nimbus.core.hud.HudSnapshot;
import dev.nimbus.core.hud.HudTelemetry;
import dev.nimbus.core.ui.ClickGui;
import dev.nimbus.core.ui.NotificationCenter;
import dev.nimbus.core.ui.Radial;

import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Offline checks for the Nimbus core model: layout, notifications, telemetry, drawing.
 *
 * Игру эти проверки не заменяют, но ловят то, что в игре видно хуже всего:
 * незакрытый scissor, потерянную раскладку, вечное уведомление и счётчик кадров,
 * который врёт на первых кадрах.
 */
public final class VerifyCoreModel {

    private static final long MS = 1_000_000L;

    private VerifyCoreModel() {
    }

    public static void main(String[] args) throws Exception {
        Path root = Files.createTempDirectory("nimbus-core-");
        try {
            checkLayout(root);
            checkNotifications();
            checkNotificationRendering();
            checkTelemetry();
            checkRendering(root);
            System.out.println("Verified Nimbus core model");
        } finally {
            deleteTree(root);
        }
    }

    // ------------------------------------------------------------------ раскладка HUD

    private static void checkLayout(Path root) throws Exception {
        Path file = root.resolve("layout").resolve("hud.properties");
        HudLayout layout = new HudLayout(file);
        layout.load();
        expect(layout.size() == 0, "новая раскладка не пуста");
        expect(!layout.has("fps"), "в новой раскладке уже есть панель");
        expect(!layout.dirty(), "новая раскладка помечена изменённой");

        layout.set("fps", 0.25f, 0.5f);
        expect(layout.dirty() && layout.size() == 1, "положение не запомнилось");
        expect(layout.saveIfDirty() && !layout.dirty() && !layout.saveFailed(), "раскладка не сохранилась");

        layout.set("fps", 0.25f, 0.5f);
        expect(!layout.dirty(), "повтор того же положения отметил файл изменённым");

        layout.set("cps", -3f, 9f);
        float[] clamped = layout.get("cps");
        expect(clamped != null && clamped[0] == 0f && clamped[1] == 1f, "положение не прижато к экрану");

        layout.set("nan", Float.NaN, 0.5f);
        layout.set("inf", 0.5f, Float.POSITIVE_INFINITY);
        layout.set(null, 0.5f, 0.5f);
        expect(layout.size() == 2, "не-число попало в раскладку");

        expect(layout.saveIfDirty(), "повторное сохранение не удалось");
        HudLayout reloaded = new HudLayout(file);
        reloaded.load();
        expect(reloaded.size() == 2, "раскладка не прочиталась обратно");
        float[] point = reloaded.get("fps");
        expect(point != null && Math.abs(point[0] - 0.25f) < 0.0005f && Math.abs(point[1] - 0.5f) < 0.0005f,
                "положение не совпало после перезагрузки");

        reloaded.reset("fps");
        expect(reloaded.size() == 1 && reloaded.dirty(), "сброс одной панели не сработал");
        reloaded.resetAll();
        expect(reloaded.size() == 0, "полный сброс не сработал");

        checkLayoutLimits(root);
        checkLayoutFailure(root);
    }

    /** Порченный, чужой и разросшийся файл раскладки. */
    private static void checkLayoutLimits(Path root) throws Exception {
        Path malformed = root.resolve("malformed.properties");
        Files.writeString(malformed,
                "fps=abc\n"
                        + "cps=0.5,0.25\n"
                        + "empty=,\n"
                        + "triple=1,2,3\n"
                        + "infinite=Infinity,0.5\n"
                        + "single=0.5\n");
        HudLayout broken = new HudLayout(malformed);
        broken.load();
        expect(broken.size() == 1 && broken.has("cps"), "испорченные строки не отброшены по одной");
        expect(!broken.saveFailed(), "читаемый файл отмечен как сбойный");

        StringBuilder huge = new StringBuilder();
        for (int i = 0; i < 400; i++) {
            huge.append("panel").append(i).append("=0.5,0.5\n");
        }
        Path many = root.resolve("many.properties");
        Files.writeString(many, huge.toString());
        HudLayout capped = new HudLayout(many);
        capped.load();
        expect(capped.size() == 256, "предел числа панелей не сработал: " + capped.size());

        // Файл больше предела: читать его в игровом потоке нельзя, играем по умолчанию.
        StringBuilder oversized = new StringBuilder();
        while (oversized.length() <= (1 << 20)) {
            oversized.append("# ").append(oversized.length()).append(" очень длинный комментарий\n");
        }
        Path big = root.resolve("big.properties");
        Files.writeString(big, oversized.toString());
        HudLayout bounded = new HudLayout(big);
        bounded.load();
        expect(bounded.size() == 0 && bounded.saveFailed(), "предел размера файла раскладки не сработал");
    }

    /** Каталог занят обычным файлом: сохранение обязано отказать заметно и обратимо. */
    private static void checkLayoutFailure(Path root) throws Exception {
        Path blocked = root.resolve("blocked");
        Files.writeString(blocked, "not a directory");
        HudLayout layout = new HudLayout(blocked.resolve("hud.properties"));
        layout.load();
        layout.set("fps", 0.5f, 0.5f);
        expect(!layout.saveIfDirty(), "сохранение в занятый каталог прошло успешно");
        expect(layout.dirty() && layout.saveFailed(), "неудачное сохранение не осталось в памяти");

        Files.delete(blocked);
        Files.createDirectories(blocked);
        expect(layout.saveIfDirty(), "сохранение не восстановилось");
        expect(!layout.dirty() && !layout.saveFailed(), "признак сбоя не снялся после успеха");
    }

    // ------------------------------------------------------------------ уведомления

    private static void checkNotifications() {
        NotificationCenter center = new NotificationCenter();
        center.push(null, NotificationCenter.Level.INFO);
        center.push("   ", NotificationCenter.Level.INFO);
        center.push("текст", null);
        expect(center.size() == 0, "пустое уведомление попало в очередь");

        long now = 0L;
        for (int i = 0; i < 6; i++) {
            center.push("плашка " + i, NotificationCenter.Level.INFO, 3_000L * MS, now);
        }
        expect(center.size() == 4, "очередь уведомлений не ограничена: " + center.size());

        int before = center.size();
        center.push("плашка 5", NotificationCenter.Level.WARNING, 3_000L * MS, now + 10 * MS);
        expect(center.size() == before, "повтор сообщения создал вторую плашку");

        center.prune(now + 2_999L * MS);
        expect(center.size() == before, "плашка исчезла раньше срока");
        center.prune(now + 3_005L * MS);
        expect(center.size() == 1, "истёкшие плашки не убраны: " + center.size());
        center.prune(now + 3_009L * MS);
        expect(center.size() == 1, "продлённая плашка убрана вместе с остальными");
        center.prune(now + 3_010L * MS);
        expect(center.size() == 0, "продлённая плашка не убралась в свой срок");

        // Нулевой срок жизни: плашка обязана дожить до конца собственного появления.
        center.push("мгновенная", NotificationCenter.Level.SUCCESS, 0L, now);
        center.prune(now + 400L * MS);
        expect(center.size() == 1, "плашка с нулевым сроком не дожила до появления");
        center.prune(now + 500L * MS);
        expect(center.size() == 0, "нижний предел срока жизни превратился в бесконечный");

        // Длинное сообщение обрезается при записи: обрезок и полная строка - одна плашка.
        StringBuilder long1 = new StringBuilder();
        while (long1.length() < 200) {
            long1.append("очень подробное сообщение о состоянии ");
        }
        String full = long1.toString();
        center.push(full, NotificationCenter.Level.WARNING, 3_000L * MS, now);
        center.push(full.substring(0, 120), NotificationCenter.Level.WARNING, 3_000L * MS, now);
        expect(center.size() == 1, "длинное сообщение не обрезано при записи: " + center.size());
    }

    /**
     * Габарит стопки уведомлений.
     *
     * Плашки стоят в правом верхнем углу и растут вниз, поэтому на низком окне
     * стопка легко уходит за нижний край, а на узком - за левый. Сколько плашек
     * влезло - дело отрисовки, но нарисованное обязано остаться на экране.
     *
     * Время здесь настоящее: render сам смотрит на таймер и вычищает истёкшее,
     * так что плашки с выдуманным моментом рождения он бы просто выбросил.
     */
    private static void checkNotificationRendering() {
        int[][] windows = {{854, 480}, {1920, 1080}, {640, 360}, {320, 240}, {200, 120}, {120, 64}};
        double[] scales = {1d, 2d, 3d};
        for (int[] window : windows) {
            for (double scale : scales) {
                NotificationCenter center = new NotificationCenter();
                // Плашка появляется за доли секунды, и на самом первом кадре она
                // ещё полностью прозрачна - отрисовка такую пропускает. Поэтому
                // моментом рождения берётся прошлое: мерить надо проявившуюся плашку.
                long birth = System.nanoTime() - 300L * MS;
                center.push("Nimbus готов", NotificationCenter.Level.SUCCESS, 3_000L * MS, birth);
                center.push("Раскладка сброшена", NotificationCenter.Level.INFO, 3_000L * MS, birth);
                center.push("Не удалось сохранить настройки", NotificationCenter.Level.WARNING, 3_000L * MS, birth);
                center.push("Счётчик кадров - вкл", NotificationCenter.Level.SUCCESS, 3_000L * MS, birth);
                expect(center.size() == 4, "очередь уведомлений не набралась");

                String stage = "уведомления " + window[0] + "x" + window[1] + " масштаб " + scale;
                RecordingBridge game = new RecordingBridge(window[0], window[1], scale);
                game.beginFrame(null, 0.5f);
                game.beginBounds();
                center.render(game, 0xFF3BA9FF);
                game.expectBoundsInside(stage);
                game.endFrame();
                game.expectBalanced(stage);
                expect(game.failure() == null, stage + ": " + game.failure());
                // Иначе проверка границ прошла бы просто потому, что рисовать было нечего.
                expect(game.fills() > 0 && game.texts() > 0, stage + ": ни одна плашка не нарисована");
            }
        }
    }

    // ------------------------------------------------------------------ телеметрия

    private static void checkTelemetry() {
        RecordingBridge game = new RecordingBridge(854, 480, 2d);
        game.setPing(-1);
        HudTelemetry telemetry = new HudTelemetry(0L);

        long now = 0L;
        HudSnapshot first = telemetry.sample(game, 0, 0, now);
        expect(first.fps() == 0, "первый кадр уже дал счётчик кадров");
        expect(!first.inWorld() && first.playerX() == 0d, "координаты вне мира не обнулены");

        // Ровно шестьдесят кадров в секунду в течение четырёх секунд.
        for (int i = 0; i < 240; i++) {
            now += 16 * MS + 666_666L;
            telemetry.sample(game, 0, 0, now);
        }
        HudSnapshot steady = telemetry.sample(game, 3, 1, now + 16 * MS);
        expect(steady.fps() >= 55 && steady.fps() <= 65, "счётчик кадров врёт: " + steady.fps());
        expect(steady.frameTimeMs() > 10f && steady.frameTimeMs() < 25f,
                "время кадра врёт: " + steady.frameTimeMs());
        expect(steady.onePercentLow() >= 55 && steady.onePercentLow() <= 65,
                "один процент худших на ровных кадрах разошёлся: " + steady.onePercentLow());
        expect(steady.leftCps() == 3 && steady.rightCps() == 1, "клики не дошли до снимка");
        expect(steady.ping() == -1, "недоступный пинг превратился в число");
        expect(steady.sessionSeconds() >= 3 && steady.sessionSeconds() <= 5,
                "время в игре посчитано неверно: " + steady.sessionSeconds());

        int steadyFps = steady.fps();
        // Пять просадок по пятой доле секунды: средний счётчик их почти не заметит,
        // а один процент худших кадров обязан провалиться - в этом весь его смысл.
        for (int i = 0; i < 5; i++) {
            now += 200 * MS;
            telemetry.sample(game, 0, 0, now);
        }
        HudSnapshot dropped = telemetry.sample(game, 0, 0, now + 16 * MS);
        expect(dropped.onePercentLow() <= 10,
                "один процент худших не заметил просадку: " + dropped.onePercentLow());
        expect(dropped.onePercentLow() < steadyFps, "один процент худших не ниже среднего счётчика");
        expect(dropped.onePercentLow() >= 1, "один процент худших ушёл в ноль");

        game.setInWorld(true);
        game.setPing(42);
        HudSnapshot world = telemetry.sample(game, 0, 0, now + 2_000L * MS);
        expect(world.inWorld(), "мир не отметился в снимке");
        expect(Math.abs(world.playerX() - 128.5d) < 0.001d, "координата X не дошла до снимка");
        expect(Math.abs(world.playerZ() + 256.25d) < 0.001d, "координата Z не дошла до снимка");
        expect(world.ping() == 42, "пинг не обновился");
        expect(world.maxMemory() > 0L, "память JVM не измерена");
    }

    // ------------------------------------------------------------------ отрисовка

    private static void checkRendering(Path root) throws Exception {
        Path settingsFile = root.resolve("render").resolve("settings.properties");
        Path layoutFile = root.resolve("render").resolve("hud.properties");
        Settings settings = new Settings(settingsFile);
        settings.load();
        enableEverything(settings);

        HudLayout layout = new HudLayout(layoutFile);
        Hud hud = new Hud(settings, layout);
        ClickGui gui = new ClickGui(settings);
        Radial radial = new Radial(settings);

        // Крайние размеры окна и все ванильные масштабы интерфейса: панели обязаны
        // остаться на экране и в узком окне, где отступы и подписи не влезают.
        int[][] windows = {{854, 480}, {1920, 1080}, {640, 360}, {320, 240}, {200, 120}};
        double[] scales = {1d, 2d, 3d};
        for (int[] window : windows) {
            for (double scale : scales) {
                RecordingBridge game = new RecordingBridge(window[0], window[1], scale);
                game.setInWorld(true);
                drawHudFrames(hud, game, 3);
                String stage = "HUD " + window[0] + "x" + window[1] + " масштаб " + scale;
                game.expectBalanced(stage);
                expect(game.failure() == null, stage + ": " + game.failure());
                expect(game.fills() > 0 && game.texts() > 0, stage + ": ничего не нарисовано");

                // Тот же кадр в главном меню: игрока нет, панели обязаны выжить.
                game.setInWorld(false);
                drawHudFrames(hud, game, 2);
                game.expectBalanced(stage + " без мира");
                expect(game.failure() == null, stage + " без мира: " + game.failure());
            }
        }

        checkMenus(gui, radial, settings);
        checkEditor(hud, layout, layoutFile);
    }

    /** Включить всё, что можно включить: так в кадре оказывается максимум панелей. */
    private static void enableEverything(Settings settings) {
        for (Module module : settings.modules()) {
            if (module.toggleable()) {
                module.setOn(true);
            }
        }
    }

    private static void drawHudFrames(Hud hud, RecordingBridge game, int frames) {
        for (int i = 0; i < frames; i++) {
            game.beginFrame(new Object(), 0.5f);
            hud.render(game, "1.20.1", false);
            game.endFrame();
        }
    }

    /** Меню настроек и круговое меню: у обоих есть прокрутка, ввод и свои границы. */
    private static void checkMenus(ClickGui gui, Radial radial, Settings settings) {
        RecordingBridge game = new RecordingBridge(1280, 720, 2d);
        game.setInWorld(true);
        gui.toggle();
        expect(gui.visible(), "меню не открылось");

        int[][] cursor = {{640, 360}, {200, 140}, {1270, 715}, {0, 0}};
        for (int[] point : cursor) {
            game.setMouse(point[0], point[1]);
            game.pushScroll(point[0] % 2 == 0 ? 3d : -5d);
            game.beginFrame(new Object(), 0.5f);
            gui.render(game, "1.20.1", 120);
            game.endFrame();
            game.expectBalanced("меню в точке " + point[0] + "," + point[1]);
        }
        expect(game.failure() == null, "меню: " + game.failure());
        expect(game.fills() > 0 && game.texts() > 0, "меню ничего не нарисовало");
        expect(!gui.capturingKey() && !gui.capturingText(), "меню осталось в режиме захвата ввода");

        // Узкое окно: заголовки и подписи обязаны обрезаться, а не выходить наружу.
        RecordingBridge narrow = new RecordingBridge(240, 180, 3d);
        narrow.setMouse(120, 90);
        narrow.beginFrame(new Object(), 0.5f);
        gui.render(narrow, "1.20.1", 60);
        narrow.endFrame();
        narrow.expectBalanced("меню в узком окне");
        expect(narrow.failure() == null, "меню в узком окне: " + narrow.failure());
        gui.close();
        expect(!gui.visible(), "меню не закрылось");

        radial.beginGesture();
        expect(radial.visible(), "круговое меню не открылось");
        for (int[] point : cursor) {
            game.setMouse(point[0], point[1]);
            game.beginFrame(new Object(), 0.5f);
            radial.render(game);
            game.endFrame();
            game.expectBalanced("круг в точке " + point[0] + "," + point[1]);
        }

        // Отпускание в мёртвой зоне не имеет права ничего переключить: круг закрыт
        // тем же движением, которым его открыли, и промах должен стоить ноль.
        game.setMouse(game.screenWidth() / 2, game.screenHeight() / 2);
        game.beginFrame(new Object(), 0.5f);
        radial.render(game);
        game.endFrame();
        boolean[] before = wheelState(settings);
        radial.endGesture();
        expectSame(before, wheelState(settings), "отпускание в мёртвой зоне переключило модуль");

        // Круг после отпускания ещё гаснет, поэтому невидимым он становится не сразу.
        radial.close();
        expect(game.failure() == null, "круговое меню: " + game.failure());
        expect(settings.modules().size() > 0, "настройки пусты");
    }

    private static boolean[] wheelState(Settings settings) {
        java.util.List<Module> wheel = settings.wheel();
        boolean[] state = new boolean[wheel.size()];
        for (int i = 0; i < state.length; i++) {
            state[i] = wheel.get(i).on();
        }
        return state;
    }

    private static void expectSame(boolean[] before, boolean[] after, String message) {
        expect(before.length == after.length, message + ": состав круга изменился");
        for (int i = 0; i < before.length; i++) {
            expect(before[i] == after[i], message + ": позиция " + i);
        }
    }

    /**
     * Редактор целиком: выбор панели, перетаскивание, сохранение и возврат.
     *
     * Именно этот путь игрок теряет болезненнее всего: расстановка двух десятков
     * плашек, которая не дожила до следующего запуска, - это работа впустую.
     */
    private static void checkEditor(Hud hud, HudLayout layout, Path layoutFile) throws Exception {
        RecordingBridge game = new RecordingBridge(854, 480, 2d);
        game.setInWorld(true);
        hud.startEditing();
        expect(hud.editing(), "редактор не открылся");

        // Первая панель стоит в безопасном отступе от края: там и берём её.
        int grab = 28;
        game.setMouse(grab, grab);
        drawHudFrames(hud, game, 2);

        game.setLeftDown(true);
        drawHudFrames(hud, game, 1);
        game.setMouse(400, 300);
        drawHudFrames(hud, game, 2);
        game.setLeftDown(false);
        drawHudFrames(hud, game, 1);

        game.expectBalanced("редактор");
        expect(game.failure() == null, "редактор: " + game.failure());
        expect(layout.size() >= 1, "перетаскивание не запомнило ни одной панели");

        // Правая кнопка возвращает панель на место по умолчанию.
        int moved = layout.size();
        game.setMouse(400, 300);
        drawHudFrames(hud, game, 1);
        game.setRightDown(true);
        drawHudFrames(hud, game, 1);
        game.setRightDown(false);
        drawHudFrames(hud, game, 1);
        expect(layout.size() < moved, "сброс панели правой кнопкой не сработал");

        hud.stopEditing();
        expect(!hud.editing(), "редактор не закрылся");
        expect(!layout.saveFailed(), "раскладка редактора не сохранилась");

        // Панель, переставленную в одном запуске, обязан увидеть следующий.
        layout.set("fps", 0.75f, 0.25f);
        expect(layout.saveIfDirty(), "сохранение раскладки редактора не удалось");
        expect(Files.isRegularFile(layoutFile), "файл раскладки не создан");
        HudLayout next = new HudLayout(layoutFile);
        next.load();
        float[] point = next.get("fps");
        expect(point != null && Math.abs(point[0] - 0.75f) < 0.0005f,
                "положение панели не дожило до следующего запуска");

        // Смена размера окна не имеет права выкинуть панель за край.
        RecordingBridge resized = new RecordingBridge(360, 240, 1d);
        resized.setInWorld(true);
        drawHudFrames(hud, resized, 2);
        resized.expectBalanced("редактор после смены размера");
        expect(resized.failure() == null, "после смены размера: " + resized.failure());
    }

    // ------------------------------------------------------------------ мелочи

    private static void expect(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }

    private static void deleteTree(Path path) throws Exception {
        if (!Files.exists(path)) {
            return;
        }
        try (var entries = Files.walk(path)) {
            entries.sorted((left, right) -> right.compareTo(left)).forEach(item -> {
                try {
                    Files.deleteIfExists(item);
                } catch (Exception error) {
                    throw new RuntimeException(error);
                }
            });
        }
    }
}
