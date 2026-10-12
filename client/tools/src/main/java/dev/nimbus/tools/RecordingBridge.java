package dev.nimbus.tools;

import dev.nimbus.bridge.GameBridge;

import java.util.ArrayDeque;
import java.util.Deque;

/**
 * Поддельная игра для офлайн-проверок отрисовки.
 *
 * Настоящую проверку интерфейса можно сделать только в игре, но часть ошибок
 * видна и без неё: незакрытый scissor гасит всё, что рисуется после него, а
 * незакрытый масштаб оставляет перекошенным весь следующий кадр. Такое ловится
 * счётом вызовов, и ловить это лучше на сборке, чем глазами в игре.
 *
 * Здесь же проверяются рамки: отрицательная ширина или высота у игры означает
 * прямоугольник, вывернутый наизнанку, и его лучше поймать здесь.
 */
final class RecordingBridge implements GameBridge {

    private final int screenWidth;
    private final int screenHeight;
    private final double guiScale;

    private final Deque<int[]> scissors = new ArrayDeque<>();
    private final Deque<Float> scales = new ArrayDeque<>();

    private int scissorDepth;
    private int scaleDepth;
    private int maxScissorDepth;
    private int maxScaleDepth;
    private int fills;
    private int texts;
    private int frames;
    private boolean insideFrame;
    private String failure;

    /**
     * Замер габарита: включается на время одного участка отрисовки.
     *
     * Мерить габарит всего кадра бессмысленно: круговое меню на крошечном окне
     * шире самого окна по своей природе, а полноэкранное затемнение занимает экран
     * целиком. Поэтому границы проверяются только там, где выход за край - ошибка.
     */
    private boolean boundsActive;
    private double minX;
    private double minY;
    private double maxX;
    private double maxY;

    private boolean inWorld;
    private boolean cursorGrabbed = true;
    private int mouseX;
    private int mouseY;
    private int ping = -1;
    private boolean leftDown;
    private boolean rightDown;
    private double scroll;

    RecordingBridge(int screenWidth, int screenHeight, double guiScale) {
        this.screenWidth = screenWidth;
        this.screenHeight = screenHeight;
        this.guiScale = guiScale;
    }

    // ------------------------------------------------------------------ разбор записи

    /** Первое нарушение или null, если всё чисто. */
    String failure() {
        return failure;
    }

    int fills() {
        return fills;
    }

    int texts() {
        return texts;
    }

    int maxScissorDepth() {
        return maxScissorDepth;
    }

    int maxScaleDepth() {
        return maxScaleDepth;
    }

    /** Все ли пары scissor и масштаба закрыты. */
    void expectBalanced(String stage) {
        if (scissorDepth != 0) {
            fail(stage + ": scissor не закрыт, глубина " + scissorDepth);
        }
        if (scaleDepth != 0) {
            fail(stage + ": масштаб не закрыт, глубина " + scaleDepth);
        }
        if (insideFrame) {
            fail(stage + ": кадр не закрыт");
        }
    }

    void resetCounters() {
        fills = 0;
        texts = 0;
    }

    /** Начать замер габарита участка. Всё, что нарисуется дальше, складывается в один бокс. */
    void beginBounds() {
        boundsActive = true;
        minX = Double.MAX_VALUE;
        minY = Double.MAX_VALUE;
        maxX = -Double.MAX_VALUE;
        maxY = -Double.MAX_VALUE;
    }

    /**
     * Закончить замер и потребовать, чтобы участок целиком остался на экране.
     *
     * Пустой участок нарушением не считается: очередь уведомлений имеет право
     * быть пустой, и тогда ей нечего рисовать.
     */
    void expectBoundsInside(String stage) {
        boundsActive = false;
        if (maxX < minX) {
            return;
        }
        if (minX < -0.5d || minY < -0.5d) {
            fail(stage + ": участок вышел за левый или верхний край: " + box());
        }
        if (maxX > screenWidth + 0.5d || maxY > screenHeight + 0.5d) {
            fail(stage + ": участок вышел за экран " + screenWidth + "x" + screenHeight + ": " + box());
        }
    }

    private String box() {
        return Math.round(minX) + "," + Math.round(minY) + ".." + Math.round(maxX) + "," + Math.round(maxY);
    }

    /**
     * Габарит одного вызова в координатах экрана.
     *
     * Внутри pushScale координаты умножены на обратный масштаб, поэтому обратно в
     * экранные они переводятся произведением всего стека: иначе плашка, нарисованная
     * в тройном разрешении, выглядела бы втрое больше экрана.
     */
    private void addBounds(int x, int y, int width, int height) {
        if (!boundsActive) {
            return;
        }
        double factor = 1d;
        for (Float scale : scales) {
            factor *= scale;
        }
        minX = Math.min(minX, x * factor);
        minY = Math.min(minY, y * factor);
        maxX = Math.max(maxX, (x + width) * factor);
        maxY = Math.max(maxY, (y + height) * factor);
    }

    void setInWorld(boolean value) {
        inWorld = value;
    }

    void setMouse(int x, int y) {
        mouseX = x;
        mouseY = y;
    }

    void setLeftDown(boolean value) {
        leftDown = value;
    }

    void setRightDown(boolean value) {
        rightDown = value;
    }

    void setPing(int value) {
        ping = value;
    }

    void pushScroll(double delta) {
        scroll += delta;
    }

    private void fail(String message) {
        if (failure == null) {
            failure = message;
        }
    }

    private void checkBox(String call, int width, int height) {
        if (width < 0 || height < 0) {
            fail(call + ": отрицательный размер " + width + "x" + height);
        }
    }

    // ------------------------------------------------------------------ мост

    @Override
    public String gameVersion() {
        return "1.20.1";
    }

    @Override
    public boolean ready() {
        return true;
    }

    @Override
    public void beginFrame(Object graphics, float partialTick) {
        if (insideFrame) {
            fail("beginFrame внутри кадра");
        }
        insideFrame = true;
        frames++;
    }

    @Override
    public void endFrame() {
        if (!insideFrame) {
            fail("endFrame без кадра");
        }
        insideFrame = false;
    }

    @Override
    public float partialTick() {
        return 0.5f;
    }

    @Override
    public int screenWidth() {
        return screenWidth;
    }

    @Override
    public int screenHeight() {
        return screenHeight;
    }

    @Override
    public double guiScale() {
        return guiScale;
    }

    @Override
    public void pushScale(float scale) {
        if (!(scale > 0f) || !Float.isFinite(scale)) {
            fail("pushScale с масштабом " + scale);
        }
        scales.push(scale);
        scaleDepth++;
        maxScaleDepth = Math.max(maxScaleDepth, scaleDepth);
    }

    @Override
    public void popScale() {
        if (scales.isEmpty()) {
            fail("popScale без pushScale");
            return;
        }
        scales.pop();
        scaleDepth--;
    }

    @Override
    public boolean cursorGrabbed() {
        return cursorGrabbed;
    }

    @Override
    public void setCursorGrabbed(boolean grabbed) {
        cursorGrabbed = grabbed;
    }

    @Override
    public boolean keyDown(int key) {
        return false;
    }

    @Override
    public int mouseX() {
        return mouseX;
    }

    @Override
    public int mouseY() {
        return mouseY;
    }

    @Override
    public double mouseXPrecise() {
        return mouseX;
    }

    @Override
    public double mouseYPrecise() {
        return mouseY;
    }

    @Override
    public boolean mouseDown(int button) {
        return button == 0 ? leftDown : button == 1 && rightDown;
    }

    @Override
    public double takeScroll() {
        double value = scroll;
        scroll = 0d;
        return value;
    }

    @Override
    public void fill(int x, int y, int width, int height, int argb) {
        checkBox("fill", width, height);
        addBounds(x, y, width, height);
        fills++;
    }

    @Override
    public void fillGradient(int x, int y, int width, int height, int topArgb, int bottomArgb) {
        checkBox("fillGradient", width, height);
        addBounds(x, y, width, height);
        fills++;
    }

    @Override
    public int drawText(String text, int x, int y, int argb, boolean shadow) {
        if (text == null) {
            fail("drawText с пустой ссылкой");
            return x;
        }
        addBounds(x, y, textWidth(text), textHeight());
        texts++;
        return x + textWidth(text);
    }

    @Override
    public int textWidth(String text) {
        // Шрифт игры пропорциональный, но для проверки границ достаточно постоянной ширины.
        return text == null ? 0 : text.length() * 6;
    }

    @Override
    public int textHeight() {
        return 9;
    }

    @Override
    public void scissorOn(int x, int y, int width, int height) {
        checkBox("scissorOn", width, height);
        scissors.push(new int[]{x, y, width, height});
        scissorDepth++;
        maxScissorDepth = Math.max(maxScissorDepth, scissorDepth);
    }

    @Override
    public void scissorOff() {
        if (scissors.isEmpty()) {
            fail("scissorOff без scissorOn");
            return;
        }
        scissors.pop();
        scissorDepth--;
    }

    @Override
    public boolean inWorld() {
        return inWorld;
    }

    @Override
    public double playerX() {
        return 128.5d;
    }

    @Override
    public double playerY() {
        return 64d;
    }

    @Override
    public double playerZ() {
        return -256.25d;
    }

    @Override
    public float playerYaw() {
        return 135f;
    }

    @Override
    public int ping() {
        return ping;
    }

    @Override
    public void playSound(int sound, float pitch) {
    }

    @Override
    public void printMessage(String text) {
    }

    @Override
    public String playerName() {
        return "Tester";
    }
}
