import android.net.NetworkCapabilities;
import android.net.NetworkRequest;
import java.lang.reflect.Modifier;

public final class NetworkRequestSmoke {
    private static int checks;

    private static void check(boolean condition, String description) {
        if (!condition) throw new AssertionError(description);
        ++checks;
    }

    private static void rejects(Runnable action, String description) {
        try {
            action.run();
        } catch (IllegalArgumentException expected) {
            ++checks;
            return;
        }
        throw new AssertionError(description);
    }

    private static NetworkRequest sdkCallChain() {
        // Actual split_GNASDK.apk constructor call chain:
        // com.ttnet.gsdk.org.chromium.net.NetworkChangeNotifierAutoDetect.
        return new NetworkRequest.Builder().addCapability(12).removeCapability(15).build();
    }

    public static void main(String[] args) throws Exception {
        check(Modifier.isStatic(NetworkRequest.Builder.class.getModifiers()), "Builder must be static");
        check(NetworkRequest.Builder.class.getConstructor().newInstance() != null,
                "Builder must expose the SDK's no-argument constructor");
        check(NetworkRequest.Builder.class.getMethod("removeCapability", int.class).getReturnType()
                == NetworkRequest.Builder.class, "removeCapability return descriptor");

        NetworkRequest sdk = sdkCallChain();
        check(sdk.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET), "SDK requests INTERNET");
        check(!sdk.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN), "SDK removes NOT_VPN");
        check(sdk.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED), "default NOT_RESTRICTED");
        check(sdk.hasCapability(NetworkCapabilities.NET_CAPABILITY_TRUSTED), "default TRUSTED");
        check(!sdk.hasTransport(NetworkCapabilities.TRANSPORT_WIFI), "no transport requested by SDK");

        NetworkRequest.Builder builder = new NetworkRequest.Builder();
        check(builder.addCapability(12) == builder, "fluent add capability");
        check(builder.addTransportType(1) == builder, "fluent add transport");
        NetworkRequest first = builder.build();
        check(builder.removeCapability(12) == builder, "fluent remove capability");
        check(builder.removeTransportType(1) == builder, "fluent remove transport");
        builder.addTransportType(0);
        NetworkRequest second = builder.build();
        check(first != second, "build creates independent requests");
        check(first.hasCapability(12), "old request retains capability");
        check(first.hasTransport(1), "old request retains transport");
        check(!first.hasTransport(0), "old request gains no later transport");
        check(!second.hasCapability(12), "new request reflects capability removal");
        check(!second.hasTransport(1) && second.hasTransport(0), "new request reflects transport change");
        builder.addCapability(12).removeTransportType(0);
        NetworkRequest third = builder.build();
        check(third.hasCapability(12) && !third.hasTransport(0), "subsequent build reflects current state");
        check(!second.hasCapability(12) && second.hasTransport(0), "second snapshot remains independent");
        check(!new NetworkRequest.Builder().build().hasCapability(12), "builders do not share state");
        check(new NetworkRequest.Builder().build().hasCapability(15), "default builder keeps NOT_VPN");

        NetworkCapabilities original = new NetworkCapabilities().removeCapability(15)
                .addCapability(33).addTransportType(9);
        NetworkCapabilities copy = new NetworkCapabilities(original);
        original.addCapability(15).removeCapability(33).removeTransportType(9).addTransportType(1);
        check(copy.hasCapability(33) && !copy.hasCapability(15), "copy retains full capability mask");
        check(copy.hasTransport(9) && !copy.hasTransport(1), "copy retains transport mask");
        NetworkCapabilities emptyCopy = new NetworkCapabilities(null);
        check(!emptyCopy.hasCapability(13) && !emptyCopy.hasCapability(14)
                && !emptyCopy.hasCapability(15), "null copy has empty capability mask");
        check(!emptyCopy.hasTransport(0), "null copy has empty transport mask");

        rejects(() -> builder.addCapability(-1), "reject negative capability");
        rejects(() -> builder.removeCapability(36), "reject unsupported capability");
        rejects(() -> builder.addTransportType(-1), "reject negative transport");
        rejects(() -> builder.removeTransportType(10), "reject unsupported transport");
        NetworkRequest afterErrors = builder.build();
        check(afterErrors.hasCapability(12) && !afterErrors.hasTransport(0), "validation errors preserve builder state");
        check(!first.hasCapability(-1) && !first.hasCapability(64), "invalid capability queries return false");
        check(!first.hasTransport(-1) && !first.hasTransport(64), "invalid transport queries return false");
        System.out.println("CLBB_NETWORK_REQUEST_OK=" + checks);
    }
}
