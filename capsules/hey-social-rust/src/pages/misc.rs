// Shared page chrome + the NotFound fallback. AppShell wraps every
// signed-in page in a sticky-header layout; the floating dock is laid
// over the content from each page that wants it.

use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    view! {
        <div class="min-h-screen bg-slate-50 dark:bg-slate-950">
            <header class="sticky top-0 z-10 border-b border-slate-200 dark:border-slate-800 bg-white/80 dark:bg-slate-950/80 backdrop-blur">
                <div class="max-w-2xl mx-auto px-4 h-14 flex items-center justify-between">
                    <A href="/home" attr:class="text-lg font-semibold tracking-tight text-slate-900 dark:text-slate-50">
                        "Hey " <span class="text-amber-500">"Social"</span>
                    </A>
                </div>
            </header>
            {children()}
        </div>
    }
}

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <section class="min-h-screen flex items-center justify-center p-8 text-center">
            <div>
                <h1 class="text-6xl font-bold text-slate-300 dark:text-slate-700">"404"</h1>
                <p class="mt-3 text-sm text-slate-600 dark:text-slate-400">"Page not found."</p>
                <A
                    href="/home"
                    attr:class="mt-6 inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-5 py-2.5 text-sm shadow-md transition-colors"
                >
                    "Go home"
                </A>
            </div>
        </section>
    }
}
