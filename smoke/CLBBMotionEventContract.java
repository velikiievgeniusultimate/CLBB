import android.view.MotionEvent;
import android.view.MotionEvent.PointerCoords;
import android.view.MotionEvent.PointerProperties;

/** Contract test against the complete production MotionEvent class, no game,
 * JNI mock, desktop input device, focus change or synthetic global mouse. */
public final class CLBBMotionEventContract {
    private static int checks;
    private static void check(boolean value, String message) {
        checks++;
        if (!value) throw new AssertionError(message);
    }
    private static MotionEvent packet(long down, long now, int action, int[] ids) {
        float[] xy = new float[ids.length * 4];
        for (int i = 0; i < ids.length; i++) {
            xy[4*i] = ids[i]*103 + .25f;
            xy[4*i+1] = ids[i]*71 + .5f;
            xy[4*i+2] = xy[4*i] + 4;
            xy[4*i+3] = xy[4*i+1] + 8;
        }
        return new MotionEvent(0x1002, action, down, now, ids, xy);
    }
    private static void verify(MotionEvent e, long down, long now, int[] expected) {
        check(e.getPointerCount() == expected.length, "pointer count");
        check(e.getDownTime() == down, "one gesture downTime across all fingers");
        check(e.getEventTime() == now, "eventTime");
        check(e.getEventTimeNano() == now*1_000_000L, "nanosecond conversion");
        int bits = 0;
        for (int i = 0; i < expected.length; i++) {
            int id = expected[i];
            bits |= 1 << id;
            check(e.getPointerId(i) == id, "stable ID after array reorder");
            check(e.findPointerIndex(id) == i, "lookup by pointer ID");
            check(e.getX(i) == id*103 + .25f, "each finger has its own X");
            check(e.getY(i) == id*71 + .5f, "each finger has its own Y");
            PointerProperties p = new PointerProperties();
            e.getPointerProperties(i, p);
            check(p.id == id && p.toolType == MotionEvent.TOOL_TYPE_FINGER, "bulk pointer identity");
            PointerCoords c = new PointerCoords();
            e.getPointerCoords(i, c);
            check(c.x == e.getX(i) && c.y == e.getY(i), "bulk pointer coordinates");
            check(c.pressure == e.getPressure(i), "bulk pressure");
        }
        check(e.getPointerIdBits() == bits, "pointer bit mask");
        check(e.findPointerIndex(14) == -1, "missing pointer ID");
        check(e.findPointerIndex(-1) == -1, "negative pointer ID");
    }
    private static void sixteenVirtualFingersAndPhysicalLeftButton() {
        final long down = 73_234_567L;
        int[] ids = new int[] {15};
        for (int count = 1; count <= 17; count++) {
            ids = new int[count];
            for (int i = 0; i < count; i++) ids[i] = i < 16 ? 15+i : 0;
            int action = count == 1 ? MotionEvent.ACTION_DOWN : MotionEvent.ACTION_POINTER_DOWN | ((count-1)<<8);
            MotionEvent e = packet(down, down+count, action, ids);
            verify(e, down, down+count, ids);
            check(e.getActionIndex() == count-1, "new finger action index");
            for (int i = 0; i < count; i++) check(e.getPressure(i) == 1, "held finger has contact pressure");
            MotionEvent copy = MotionEvent.obtain(e);
            verify(copy, down, down+count, ids);
            e.setDownTime(down-5);
            check(copy.getDownTime() == down, "copied gesture clock independent");
        }
        long now = down+100;
        // Release index 0 while other fingers remain. Production native code
        // uses remove_index_fast, so the last pointer moves into the gap.
        while (ids.length > 0) {
            int action = ids.length == 1 ? MotionEvent.ACTION_UP : MotionEvent.ACTION_POINTER_UP;
            MotionEvent e = packet(down, now++, action, ids);
            verify(e, down, now-1, ids);
            check(e.getPressure(0) == 0, "only lifted pointer loses pressure");
            for (int i = 1; i < ids.length; i++) check(e.getPressure(i) == 1, "other fingers stay held on pointer up");
            MotionEvent lifted = e.split(1 << ids[0]);
            check(lifted.getActionMasked() == MotionEvent.ACTION_UP, "split lifted finger becomes final UP");
            if (ids.length > 1) {
                MotionEvent others = e.split(e.getPointerIdBits() & ~(1 << ids[0]));
                check(others.getActionMasked() == MotionEvent.ACTION_MOVE, "unrelated UP does not release remaining fingers");
                check(others.getDownTime() == down, "split gesture retains downTime");
                check(others.findPointerIndex(ids[0]) == -1, "split removed finger absent");
            }
            int[] next = new int[ids.length-1];
            System.arraycopy(ids, 0, next, 0, next.length);
            if (next.length > 0) next[0] = ids[ids.length-1];
            ids = next;
        }
    }
    private static void copiesAndFactoryInputs() {
        PointerProperties[] pp = PointerProperties.createArray(2);
        PointerCoords[] pc = PointerCoords.createArray(2);
        pp[0].id=31; pp[0].toolType=MotionEvent.TOOL_TYPE_STYLUS;
        pp[1].id=0; pp[1].toolType=MotionEvent.TOOL_TYPE_FINGER;
        pc[0].x=4000; pc[0].y=3111; pc[0].pressure=.75f;
        pc[1].x=21; pc[1].y=61; pc[1].pressure=.5f;
        MotionEvent e=MotionEvent.obtain(100, 200, MotionEvent.ACTION_POINTER_DOWN|256, 2, pp, pc, 0, 0, 1, 1, 1, 0, 0x1002, 0);
        check(e.getDownTime()==100 && e.getEventTime()==200, "factory retains supplied clocks");
        check(e.getPointerIdBits()==((1<<31)|1), "signed bit31 pointer mask");
        check(e.getToolType(0)==MotionEvent.TOOL_TYPE_STYLUS && e.getPressure(0)==.75f, "factory retains supplied properties");
        MotionEvent split=e.split(1);
        check(split.getPointerId(0)==0 && split.getActionMasked()==MotionEvent.ACTION_DOWN, "split secondary DOWN becomes primary DOWN");
        MotionEvent copy=MotionEvent.obtainNoHistory(e);
        check(copy.getToolType(0)==MotionEvent.TOOL_TYPE_STYLUS && copy.getPressure(1)==.5f, "copy retains properties and pressure");
        MotionEvent single=MotionEvent.obtain(100, 200, MotionEvent.ACTION_DOWN, 7, 9, .3f, 0, 0, 1, 1, 1, 0);
        check(single.getPointerId(0)==0 && single.getDownTime()==100 && single.getPressure()==.3f, "single factory primary ID and explicit pressure");
        MotionEvent c=packet(100, 200, MotionEvent.ACTION_CANCEL, new int[]{15,16});
        check(c.getPressure(0)==0 && c.getPressure(1)==0, "cancel ends all contacts");
        boolean invalid=false;
        try { packet(100,200,MotionEvent.ACTION_MOVE,new int[]{15,15}); }
        catch(IllegalArgumentException ex) { invalid=true; }
        check(invalid, "duplicate pointer IDs rejected");
    }
    public static void main(String[] args) {
        sixteenVirtualFingersAndPhysicalLeftButton();
        copiesAndFactoryInputs();
        System.out.println("MOTION_EVENT_CONTRACT_OK checks="+checks+" virtual_fingers=16 physical_lmb=1 first_pointer_up_preserves_others=1");
    }
}
