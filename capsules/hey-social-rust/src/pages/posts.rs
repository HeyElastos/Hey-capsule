// Posts — create a new post (caption + photo/video). Rust port of
// capsules/hey-social/client/src/pages/Posts.jsx (293 lines of React).
//
// Flow:
//   1. User picks a file via <input type="file">.
//   2. Read it as ArrayBuffer → Uint8Array.
//   3. POST to /api/provider/ipfs/add_bytes (via runtime::ipfs::add_bytes).
//   4. Call api::posts::create_post with caption + the returned CID.
//
// What's not wired up yet: the hey-transcoder pre-processing pipeline (the
// React version normalizes images to WebP @ 2048px and videos to H.264 @
// 1080p / CRF 23 before pinning). We upload as-is until the transcoder
// provider call gets ported.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, HtmlInputElement};

use crate::api::posts::{create_post, ipfs_upload_media, CreatePostArgs, MediaTile};
use crate::components::icons::{CameraIcon, ImageIcon};
use crate::components::{FloatingDock, TopHeader};

#[component]
pub fn Posts() -> impl IntoView {
    let caption = RwSignal::new(String::new());
    let staged: RwSignal<Option<StagedFile>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let progress = RwSignal::new(0u32);
    let error = RwSignal::new(String::new());
    let navigate = use_navigate();

    let on_file_change = move |ev: Event| {
        let target = match ev.target() {
            Some(t) => t,
            None => return,
        };
        let input: HtmlInputElement = match target.dyn_into() {
            Ok(el) => el,
            Err(_) => return,
        };
        let files = match input.files() {
            Some(fl) => fl,
            None => return,
        };
        if files.length() == 0 {
            return;
        }
        let file = files.get(0).unwrap();
        let name = file.name();
        let mime = file.type_();
        error.set(String::new());
        // Read into bytes.
        spawn_local(async move {
            let buf_promise = file.array_buffer();
            let buf_value = match JsFuture::from(buf_promise).await {
                Ok(v) => v,
                Err(_) => {
                    error.set("Couldn't read that file.".into());
                    return;
                }
            };
            let array = js_sys::Uint8Array::new(&buf_value);
            let mut bytes = vec![0u8; array.length() as usize];
            array.copy_to(&mut bytes);
            staged.set(Some(StagedFile {
                bytes,
                name,
                mime,
            }));
        });
    };

    let submit = move |_| {
        if busy.get() {
            return;
        }
        let Some(file) = staged.get() else {
            error.set("Pick a photo or video first.".into());
            return;
        };
        let cap = caption.get();
        let navigate = navigate.clone();
        error.set(String::new());
        busy.set(true);
        progress.set(10);
        spawn_local(async move {
            // 1. Upload media to IPFS.
            let media: MediaTile = match ipfs_upload_media(&file.bytes, &file.name, &file.mime).await
            {
                Ok(m) => m,
                Err(e) => {
                    error.set(format!("IPFS upload failed: {e}"));
                    busy.set(false);
                    progress.set(0);
                    return;
                }
            };
            progress.set(80);
            // 2. Create the local post record.
            match create_post(CreatePostArgs {
                caption: cap,
                images: vec![media],
            })
            .await
            {
                Ok(_) => {
                    progress.set(100);
                    busy.set(false);
                    navigate("/home", NavigateOptions::default());
                }
                Err(e) => {
                    error.set(format!("Couldn't save post: {e}"));
                    busy.set(false);
                    progress.set(0);
                }
            }
        });
    };

    view! {
        <>
            <TopHeader />
            <FloatingDock />
            <div class="relative mx-auto max-w-3xl space-y-6 px-4 pt-6 pb-32 sm:px-6 sm:py-10 md:pl-32">
                <header class="px-1 animate-fade-in">
                    <h1 class="logo-handwritten text-4xl text-primary sm:text-5xl">
                        "Share a moment"
                    </h1>
                    <p class="mt-1 text-sm text-muted">
                        "Photo or short video. Stored on IPFS, pinned to your node, federated to your followers."
                    </p>
                </header>

                <div class="frosted-card p-6 space-y-4 animate-fade-up">
                    <label class="block">
                        <span class="text-[11px] uppercase tracking-wider text-muted">
                            "Media"
                        </span>
                        <div class="mt-2 flex items-center gap-3">
                            <label class="cursor-pointer inline-flex items-center gap-2 rounded-full bg-white/10 hover:bg-white/20 border border-surface px-4 py-2 text-sm font-medium text-primary">
                                <ImageIcon class="h-4 w-4" />
                                "Choose file"
                                <input
                                    type="file"
                                    class="sr-only"
                                    accept="image/*,video/*"
                                    on:change=on_file_change
                                />
                            </label>
                            {move || {
                                let s = staged.read();
                                match s.as_ref() {
                                    Some(f) => view! {
                                        <span class="text-xs text-muted truncate">
                                            {f.name.clone()} " (" {f.bytes.len().to_string()} " bytes)"
                                        </span>
                                    }.into_any(),
                                    None => view! {
                                        <span class="text-xs text-muted">
                                            "No file chosen"
                                        </span>
                                    }.into_any(),
                                }
                            }}
                        </div>
                    </label>

                    <label class="block">
                        <span class="text-[11px] uppercase tracking-wider text-muted">
                            "Caption"
                        </span>
                        <textarea
                            class="frosted-input mt-2 text-sm"
                            rows="3"
                            maxlength="2200"
                            placeholder="Say something…"
                            on:input=move |ev: web_sys::Event| {
                                let target = ev.target().unwrap();
                                let ta = target
                                    .dyn_into::<web_sys::HtmlTextAreaElement>()
                                    .unwrap();
                                caption.set(ta.value());
                            }
                        />
                    </label>

                    {move || {
                        let p = progress.get();
                        if p == 0 { view! { <></> }.into_any() }
                        else {
                            view! {
                                <div class="h-1.5 w-full overflow-hidden rounded-full bg-white/10">
                                    <div class="h-full bg-accent transition-[width] duration-300" style=move || format!("width: {}%", progress.get())></div>
                                </div>
                            }.into_any()
                        }
                    }}

                    {move || {
                        let msg = error.get();
                        if msg.is_empty() { view! { <></> }.into_any() }
                        else {
                            view! { <p class="text-sm text-red-400">{msg}</p> }.into_any()
                        }
                    }}

                    <button
                        type="button"
                        on:click=submit
                        prop:disabled=move || busy.get()
                        class="unfrost w-full inline-flex items-center justify-center gap-2 rounded-full bg-accent px-6 py-3 text-sm font-semibold text-accent-text shadow-lg transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-60"
                    >
                        <CameraIcon class="h-4 w-4" />
                        {move || if busy.get() { "Posting…" } else { "Post" }}
                    </button>
                </div>
            </div>
        </>
    }
}

#[derive(Clone, Debug)]
struct StagedFile {
    bytes: Vec<u8>,
    name: String,
    mime: String,
}
