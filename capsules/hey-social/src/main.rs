use hey_core::ctx::{init, CapsuleCtx};
use hey_social::App;

fn main() {
    console_error_panic_hook::set_once();

    // Install hey-social's per-capsule context for the shared hey-core engine.
    // MUST run before anything touches an engine-backed module (session today;
    // runtime/passkey as the migration proceeds). These values mirror the
    // constants hey-social's own runtime.rs has always used, so engine-backed
    // session reads/writes hit the exact same localStorage keys — no behavior
    // change, no re-login. hey-messenger supplies its own distinct set.
    init(CapsuleCtx {
        capsule_id: "hey-social",
        private_namespace: "Hey",
        session_key: "hey-social-session",
        welcomed_key: "hey-social-welcomed",
        session_redeemed_key: "hey-session-redeemed",
        home_launch_token_key: "hey-home-launch-token",
        runtime_token_key: "hey-runtime-token",
        token_store_key: "hey-capability-tokens",
        route_mode_key: "hey-storage-route-mode",
        boot_capabilities: &[
            ("elastos://peer/*", "message"),
            ("elastos://content/*", "write"),
            ("elastos://did/*", "read"),
            ("elastos://hey-transcoder/*", "execute"),
            ("elastos://elacity/*", "execute"),
        ],
    });

    leptos::mount::mount_to_body(App);
}
