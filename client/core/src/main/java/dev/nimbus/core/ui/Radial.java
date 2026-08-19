package dev.nimbus.core.ui;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Sounds;
import dev.nimbus.core.Settings;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.render.Colors;
import dev.nimbus.core.render.Draw;
import dev.nimbus.core.render.Easing;
import dev.nimbus.core.render.Pixels;
import dev.nimbus.core.render.Ring;

import java.util.List;

/**
 * Быстрое колесо.
 *
 * Полное меню - это остановка игры. Но в бою нужно не меню, а одно движение:
 * зажать клавишу, дёрнуть мышь в сторону нужного сектора и отпустить. Колесо
 * умеет ровно одну вещь - переключать шесть часто нужных модулей, и именно поэтому
 * его не надо читать глазами: позиция сектора запоминается рукой.
 *
 * Звуки копятся в очереди: ввод может прийти вне кадра, а игра ждёт вызовов
 * только из своего потока отрисовки.
 */
public final class Radial {

    private static final int SECTORS = 6;
    private static final float SWEEP = 360f / SECTORS;
    private static final float GAP = 3.5f;
    private static final float INNER = 34f;
    private static final float OUTER = 66f;
    private static final float HOVER_GROW = 6f;
    private static final int DEAD_ZONE = 18;

    /** Короткие подписи: в сектор шириной в палец полное название не влезает. */
    private static final String[][] SHORT = {
            {"fps", "FPS"},
            {"cps", "CPS"},
            {"coords", "XYZ"},
            {"clock", "Часы"},
            {"keystrokes", "Клавиши"},
            {"crosshair", "Прицел"}
    };

    private final Settings settings;

    private boolean held;
    private float progress;
    private long lastFrame;
    private int hovered = -1;
    private final float[] glow = new float[SECTORS];

    private final int[] soundQueue = new int[8];
    private final float[] pitchQueue = new float[8];
    private int queued;

    public Radial(Settings settings) {
        this.settings = settings;
    }

    public boolean visible() {
        return held || progress > 0.004f;
    }

    /** Клавиша колеса зажата. */
    public void beginGesture() {
        if (held) {
            return;
        }
        held = true;
        hovered = -1;
        queueSound(Sounds.TONE, 0.95f);
    }

    /** Клавиша отпущена: выбор применяется только здесь. */
    public void endGesture() {
        if (!held) {
            return;
        }
        held = false;
        List<Module> wheel = settings.wheel();
        if (hovered >= 0 && hovered < wheel.size()) {
            Module module = wheel.get(hovered);
            module.toggle();
            queueSound(Sounds.CLICK, module.on() ? 1.25f : 0.9f);
            settings.saveIfDirty();
        }
        hovered = -1;
    }

    public void close() {
        held = false;
        hovered = -1;
    }

    public void render(GameBridge game) {
        long now = System.nanoTime();
        float delta = lastFrame == 0L ? 0f : Math.min(0.1f, (now - lastFrame) / 1_000_000_000f);
        lastFrame = now;

        flushSounds(game);

        float speed = Math.max(0.25f, settings.speed());
        progress = Easing.approach(progress, held ? 1f : 0f, 0.055f / speed, delta);
        if (!held && progress < 0.004f) {
            progress = 0f;
            return;
        }

        float ease = Easing.outBack(progress);
        float alpha = Math.min(1f, progress * 1.4f);
        int centerX = game.screenWidth() / 2;
        int centerY = game.screenHeight() / 2;

        if (held) {
            hovered = pick(game.mouseX() - centerX, game.mouseY() - centerY);
        }

        List<Module> wheel = settings.wheel();
        int accent = settings.accent();
        int surface = settings.surfaceIndex();
        int s = Pixels.scale(game);

        game.pushScale(1f / s);
        float cx = centerX * s;
        float cy = centerY * s;
        float inner = INNER * ease * s;
        float outer = OUTER * ease * s;

        // Подложка под кольцом: без неё сектора пропадают на светлом небе и снеге.
        Ring.disc(game, cx, cy, outer + 4f * s, Colors.fade(0xFF05070B, alpha * 0.35f));

        for (int i = 0; i < SECTORS && i < wheel.size(); i++) {
            Module module = wheel.get(i);
            boolean over = i == hovered;
            glow[i] = Easing.approach(glow[i], over ? 1f : 0f, 0.05f / speed, delta);

            float grow = HOVER_GROW * glow[i] * s;
            float start = i * SWEEP - SWEEP / 2f + GAP / 2f;
            float sweep = SWEEP - GAP;

            int base = Colors.mix(Theme.card(surface), Theme.cardHover(surface), glow[i]);
            if (module.on()) {
                base = Colors.mix(base, Colors.withAlpha(accent, 0xE0), 0.55f + 0.45f * glow[i]);
            }
            Ring.sector(game, cx, cy, inner, outer + grow, start, sweep, Colors.fade(base, alpha));

            if (module.on()) {
                // Тонкая внешняя дуга у включённого: состояние видно без чтения подписей.
                Ring.sector(
                        game,
                        cx,
                        cy,
                        outer + grow - 2f * s,
                        outer + grow,
                        start,
                        sweep,
                        Colors.fade(settings.accentLight(), alpha)
                );
            }
        }

        Ring.disc(game, cx, cy, inner - 3f * s, Colors.fade(Theme.base(surface), alpha * 0.96f));
        Ring.ring(game, cx, cy, inner - 3f * s, Math.max(1f, s * 0.5f), Colors.fade(Theme.LINE, alpha));
        game.popScale();

        // Подписи секторов.
        float labelRadius = (INNER + OUTER) / 2f * ease;
        for (int i = 0; i < SECTORS && i < wheel.size(); i++) {
            Module module = wheel.get(i);
            double angle = Math.toRadians(i * SWEEP);
            int labelX = centerX + (int) Math.round(Math.sin(angle) * labelRadius);
            int labelY = centerY - (int) Math.round(Math.cos(angle) * labelRadius) - game.textHeight() / 2;
            int color = module.on() ? 0xFF0B0E14 : Theme.TEXT_DIM;
            Draw.textCentered(game, shortTitle(module), labelX, labelY, Colors.fade(color, alpha), false);
        }

        // Центр: что именно сейчас переключится.
        if (hovered >= 0 && hovered < wheel.size()) {
            Module module = wheel.get(hovered);
            Draw.textCentered(game, shortTitle(module), centerX, centerY - game.textHeight() - 1, Colors.fade(Theme.TEXT, alpha), false);
            Draw.textCentered(
                    game,
                    module.on() ? "выключить" : "включить",
                    centerX,
                    centerY + 2,
                    Colors.fade(module.on() ? Theme.DANGER : Colors.withAlpha(accent, 0xFF), alpha),
                    false
            );
        } else {
            Draw.textCentered(game, "NIMBUS", centerX, centerY - game.textHeight() / 2, Colors.fade(Theme.TEXT_MUTED, alpha), false);
        }
    }

    /** Какой сектор выбран жестом. В мёртвой зоне в центре - никакой. */
    private static int pick(int dx, int dy) {
        if (dx * dx + dy * dy < DEAD_ZONE * DEAD_ZONE) {
            return -1;
        }
        double angle = Math.toDegrees(Math.atan2(dx, -dy));
        double normalized = (angle + SWEEP / 2f + 360.0) % 360.0;
        return (int) (normalized / SWEEP) % SECTORS;
    }

    private static String shortTitle(Module module) {
        for (int i = 0; i < SHORT.length; i++) {
            if (SHORT[i][0].equals(module.key())) {
                return SHORT[i][1];
            }
        }
        return module.title();
    }

    private void queueSound(int sound, float pitch) {
        if (!settings.sound() || queued >= soundQueue.length) {
            return;
        }
        soundQueue[queued] = sound;
        pitchQueue[queued] = pitch;
        queued++;
    }

    private void flushSounds(GameBridge game) {
        for (int i = 0; i < queued; i++) {
            game.playSound(soundQueue[i], pitchQueue[i]);
        }
        queued = 0;
    }
}
