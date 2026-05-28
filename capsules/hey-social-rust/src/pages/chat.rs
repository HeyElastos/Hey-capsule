// Chat — currently a placeholder. The React reference at
// capsules/hey-social/client/src/pages/Chat.jsx is 1428 lines of E2E
// crypto (ML-KEM-768 + X25519 hybrid via @noble/post-quantum) on top of
// peer.publish/recv. Porting it requires:
//   - ML-KEM-768 implementation in Rust (no current crate works in wasm
//     without significant code-size cost — pqcrypto-mlkem needs verification).
//   - X25519 (have ed25519-compact, no x25519 yet).
//   - Group key exchange + ratchet state machine.
//
// For now we point users at Hey Messenger (the standalone messaging
// capsule that already has the E2E layer in JS) and document the gap.
// Pull-request welcome — the protocol spec lives in the React file.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::FloatingDock;
use crate::pages::misc::AppShell;

#[component]
pub fn Chat() -> impl IntoView {
    view! {
        <AppShell>
            <div class="mx-auto max-w-2xl px-4 pt-6 pb-28">
                <div class="rounded-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 p-8 text-center">
                    <h2 class="text-2xl font-bold text-slate-900 dark:text-slate-50">
                        "Chat lives in Hey Messenger"
                    </h2>
                    <p class="mt-3 text-sm text-slate-600 dark:text-slate-400 max-w-md mx-auto">
                        "End-to-end-encrypted messaging in this Rust port is still being ported (the React reference uses ML-KEM-768 + X25519 hybrid post-quantum crypto — that lives in 1400 lines of JS). Use the standalone Hey Messenger capsule, same passkey, same DID."
                    </p>
                    <A
                        href="../hey-messenger/"
                        attr:class="mt-6 inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-5 py-2.5 text-sm shadow-md transition-colors"
                    >
                        "Open Hey Messenger"
                    </A>
                </div>
            </div>
            <FloatingDock />
        </AppShell>
    }
}
