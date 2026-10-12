package dev.nimbus.core.ui;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.core.render.Colors;
import dev.nimbus.core.render.Draw;
import dev.nimbus.core.render.Pixels;
import dev.nimbus.core.render.Shapes;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/** Ограниченная очередь локальных уведомлений Nimbus. */
public final class NotificationCenter {

    public enum Level {
        INFO,
        SUCCESS,
        WARNING;

        /**
         * Цвет полосы и рамки плашки.
         *
         * У обычного сообщения своего цвета нет: он берётся из акцента темы, и тогда
         * смена оформления видна и в уведомлениях. У успеха и предупреждения цвет
         * свой: их смысл не должен меняться вместе с темой - зелёное остаётся
         * зелёным даже при зелёном акценте.
         */
        int color(int accent) {
            if (this == SUCCESS) {
                return Theme.GOOD;
            }
            return this == WARNING ? Theme.WARN : accent;
        }
    }

    private static final int MAX_NOTIFICATIONS = 4;
    private static final long DEFAULT_TTL = 3_000_000_000L;

    /**
     * Меньше этого времени уведомление не живёт.
     *
     * Появление и угасание сами занимают доли секунды: с нулевым временем жизни
     * плашка мигнула бы в одном кадре и исчезла, то есть игрок её не прочитал бы.
     */
    private static final long MIN_TTL = 500_000_000L;

    /**
     * Предел длины сообщения.
     *
     * Рисуется всё равно только то, что влезло в плашку, а вот храниться могла бы
     * строка любой длины: источником бывает и текст настройки, и путь к файлу.
     */
    private static final int MAX_MESSAGE = 120;

    private final List<Notice> notices = new ArrayList<>(MAX_NOTIFICATIONS);

    public void push(String message, Level level) {
        push(message, level, DEFAULT_TTL);
    }

    public void push(String message, Level level, long ttlNanos) {
        push(message, level, ttlNanos, System.nanoTime());
    }

    /**
     * Добавить уведомление с явным моментом времени.
     *
     * Время параметром, а не из таймера: иначе проверить срок жизни и склейку
     * повторов можно было бы только настоящим ожиданием в три секунды.
     */
    public void push(String message, Level level, long ttlNanos, long nowNanos) {
        if (message == null || level == null) {
            return;
        }
        String clean = message.trim();
        if (clean.isEmpty()) {
            return;
        }
        if (clean.length() > MAX_MESSAGE) {
            // Обрезок подрезается ещё раз: иначе на срез может попасть пробел, и
            // одно и то же сообщение считалось бы двумя разными плашками.
            clean = clean.substring(0, MAX_MESSAGE).trim();
            if (clean.isEmpty()) {
                return;
            }
        }
        long expiresAt = nowNanos + Math.max(MIN_TTL, ttlNanos);
        for (int i = notices.size() - 1; i >= 0; i--) {
            Notice notice = notices.get(i);
            if (notice.message.equals(clean)) {
                // Повтор не плодит плашки: продлеваем ту же и обновляем её уровень.
                notice.createdAt = nowNanos;
                notice.expiresAt = expiresAt;
                notice.level = level;
                return;
            }
        }
        while (notices.size() >= MAX_NOTIFICATIONS) {
            notices.remove(0);
        }
        notices.add(new Notice(clean, level, nowNanos, expiresAt));
    }

    /** Сколько плашек сейчас в очереди. Нужно офлайн-проверке. */
    public int size() {
        return notices.size();
    }

    /** Убрать всё, чей срок вышел. */
    public void prune(long nowNanos) {
        for (Iterator<Notice> iterator = notices.iterator(); iterator.hasNext();) {
            if (iterator.next().expiresAt <= nowNanos) {
                iterator.remove();
            }
        }
    }

    public void render(GameBridge game, int accent) {
        if (notices.isEmpty()) {
            return;
        }
        long now = System.nanoTime();
        prune(now);
        if (notices.isEmpty()) {
            return;
        }

        int scale = Pixels.scale(game);
        int textHeight = game.textHeight();
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();
        int y = 10;
        for (int i = notices.size() - 1; i >= 0; i--) {
            Notice notice = notices.get(i);
            float life = Math.min(1f, Math.max(0f, (notice.expiresAt - now) / 300_000_000f));
            float appear = Math.min(1f, Math.max(0f, (now - notice.createdAt) / 180_000_000f));
            float alpha = Math.min(life, appear);
            String text = Shapes.clip(game, notice.message, Math.min(220, screenWidth - 44));
            int width = game.textWidth(text) + 28;
            int height = textHeight + 12;
            if (y + height > screenHeight - 10) {
                // Ниже места нет. Обрезанная по краю плашка выглядит как ошибка
                // отрисовки, поэтому лишние просто ждут своей очереди: срок жизни
                // у них короткий, и через секунду место освободится.
                break;
            }
            int x = Math.max(4, screenWidth - 10 - width);
            int top = y;
            int color = notice.level.color(accent);

            game.pushScale(1f / scale);
            Draw.roundedRect(game, x * scale, top * scale, width * scale, height * scale, 6 * scale,
                    Colors.fade(0xF20B111B, alpha));
            Draw.roundedOutline(game, x * scale, top * scale, width * scale, height * scale, 6 * scale,
                    Math.max(1, scale / 2), Colors.fade(Colors.withAlpha(color, 0xB8), alpha));
            Draw.roundedRect(game, (x + 5) * scale, (top + 5) * scale, Math.max(1, scale), (height - 10) * scale,
                    Math.max(1, scale / 2), Colors.fade(color, alpha));
            game.popScale();
            game.drawText(text, x + 13, top + 6, Colors.fade(Theme.TEXT, alpha), true);
            y += height + 5;
        }
    }

    private static final class Notice {
        private final String message;
        private Level level;
        private long createdAt;
        private long expiresAt;

        private Notice(String message, Level level, long createdAt, long expiresAt) {
            this.message = message;
            this.level = level;
            this.createdAt = createdAt;
            this.expiresAt = expiresAt;
        }
    }
}
